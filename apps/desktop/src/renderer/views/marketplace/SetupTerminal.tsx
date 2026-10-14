import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { useEffect, useRef, useState } from 'react'
import { fromBase64, HostError, toBase64 } from '../../client/host-client'
import { Button } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import type { Backend } from '@shared/models'
import { errorText, market, type AuthMethod, type SetupStep } from './market-models'
import { ScreenHeader } from './pieces'

const openLink = (uri: string) => {
  if (/^https?:/i.test(uri)) window.codync.app.openExternal(uri)
}

/**
 * Installing or signing in to an agent, or adding Node.js, live: the command runs in a
 * terminal on the computer and this screen is that terminal (links open here).
 */
export function SetupTerminalView({ backend, step, method, back }: {
  /** Null only for the `node` step, which belongs to the computer. */
  backend: Backend | null
  step: SetupStep
  /** A terminal sign-in method the agent offered; null runs Codync's own command. */
  method?: AuthMethod | null
  /** Back to the agent's sheet (the terminal is a step inside it). */
  back: () => void
}) {
  const store = useStore()
  const surface = useRef<HTMLDivElement>(null)
  const [exitCode, setExitCode] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const el = surface.current
    if (!el) return
    const css = getComputedStyle(document.documentElement)
    const scale = parseFloat(css.getPropertyValue('--scale')) || 1
    const term = new Terminal({
      fontFamily: css.getPropertyValue('--mono').trim() || 'monospace',
      fontSize: 12 * scale,
      cursorBlink: true,
      allowTransparency: true,
      theme: {
        background: css.getPropertyValue('--background').trim(),
        foreground: css.getPropertyValue('--text').trim(),
        cursor: css.getPropertyValue('--text').trim(),
        selectionBackground: `rgba(${css.getPropertyValue('--text-rgb').trim()}, 0.2)`,
      },
      linkHandler: { activate: (_e, uri) => openLink(uri) },
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.loadAddon(new WebLinksAddon((_e, uri) => openLink(uri)))
    term.open(el)
    fit.fit()
    term.focus()

    let cancelled = false
    let exited: number | null = null
    let termId: string | null = null
    let size = { cols: term.cols, rows: term.rows }
    let sent = { cols: 0, rows: 0 }
    let stopStream: (() => void) | null = null
    // One sender, so keystrokes can't overtake each other.
    let typing = Promise.resolve()
    const encoder = new TextEncoder()

    const syncSize = () => {
      const client = store.client
      if (!client || !termId || (size.cols === sent.cols && size.rows === sent.rows)) return
      sent = size
      void market.termResize(client, termId, size.cols, size.rows).catch(() => {})
    }
    const type = (bytes: Uint8Array) => {
      const client = store.client
      if (exited !== null || !client || !termId) return
      const id = termId
      typing = typing.then(() => market.termInput(client, id, toBase64(bytes)).then(() => {}, () => {}))
    }
    const subs = [
      term.onData((d) => type(encoder.encode(d))),
      term.onBinary((d) => type(Uint8Array.from(d, (c) => c.charCodeAt(0) & 0xff))),
      term.onResize(({ cols, rows }) => {
        size = { cols, rows }
        syncSize()
      }),
    ]
    const observer = new ResizeObserver(() => fit.fit())
    observer.observe(el)

    let attempt = 0
    /** Attaches to the output until it ends: `retry` after a dropped connection, `done` otherwise. */
    const attach = (client: NonNullable<typeof store.client>, id: string) =>
      new Promise<'retry' | 'done'>((resolve) => {
        term.reset()
        stopStream = client.transport.stream(
          `/term/${encodeURIComponent(id)}`,
          (line) => {
            attempt = 0
            let raw: { type: string; data?: string; code?: number }
            try {
              raw = JSON.parse(line)
            } catch {
              return
            }
            if (raw.type === 'output' && raw.data) term.write(fromBase64(raw.data))
            else if (raw.type === 'exit') {
              exited = raw.code ?? -1
              setExitCode(exited)
            }
          },
          (e) => {
            stopStream = null
            if (e?.status === 404) {
              setError(e.message)
              resolve('done')
            } else if (e) {
              attempt += 1
              if (attempt > 5) {
                setError(e.message)
                resolve('done')
              } else resolve('retry')
            } else resolve('retry')
          },
        )
      })

    void (async () => {
      try {
        const client = await store.ready()
        const id = await market.agentSetup(client, backend?.id ?? '', step, method?.id ?? null, size.cols, size.rows)
        if (cancelled) {
          void market.termClose(client, id).catch(() => {})
          return
        }
        termId = id
        sent = size
        syncSize()
        // Reattach after a dropped connection; the host replays the whole scrollback.
        while (exited === null && !cancelled) {
          if ((await attach(client, id)) === 'done') break
          if (exited === null && !cancelled) await new Promise((r) => setTimeout(r, 1000))
        }
      } catch (e) {
        if (!cancelled) setError(e instanceof HostError ? e.message : errorText(e))
        return
      }
      if (!cancelled) await store.refreshBackends()
    })()

    // Leaving the screen ends whatever is still running.
    return () => {
      cancelled = true
      observer.disconnect()
      subs.forEach((s) => s.dispose())
      stopStream?.()
      const client = store.client
      if (exited === null && client && termId) void market.termClose(client, termId).catch(() => {})
      term.dispose()
    }
  }, [store, backend?.id, step, method?.id])

  const name = backend?.name ?? 'the agent'
  const title = step === 'node' ? 'Add Node.js' : step === 'install' ? `Install ${name}` : (method?.name ?? `Sign in to ${name}`)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <ScreenHeader title={title} onBack={back} />
      <div className="setup-terminal">
        <div ref={surface} style={{ width: '100%', height: '100%' }} />
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '10px 16px', ...font('footnote') }}>
        {error ? (
          <span style={{ color: 'var(--danger)', ...clamp2 }}>{error}</span>
        ) : exitCode !== null ? (
          <>
            <Icon name={exitCode === 0 ? 'checkmark.circle.fill' : 'exclamationmark.circle.fill'} size={10} color={exitCode === 0 ? 'var(--added)' : 'var(--warning)'} />
            <span style={{ color: 'var(--secondary)', ...clamp2 }}>{exitCode === 0 ? 'Finished.' : `Ended with an error (${exitCode}).`}</span>
          </>
        ) : (
          <>
            <ThinkingOrb size={14} color="var(--secondary)" />
            <span style={{ color: 'var(--secondary)', ...clamp2 }}>Running on {store.hostName}. Links open on this device.</span>
          </>
        )}
        <span style={{ flex: 1 }} />
        {exitCode !== null ? <Button className="fade-in" onClick={back}>Done</Button> : null}
      </div>
    </div>
  )
}

const clamp2 = { display: '-webkit-box', WebkitLineClamp: 2, WebkitBoxOrient: 'vertical', overflow: 'hidden' } as const
