import { FitAddon } from '@xterm/addon-fit'
import { WebLinksAddon } from '@xterm/addon-web-links'
import { Terminal } from '@xterm/xterm'
import '@xterm/xterm/css/xterm.css'
import { useEffect, useRef, useState, type RefObject } from 'react'
import { fromBase64, HostError, toBase64, type HostClient } from '../client/host-client'
import { useDark } from '../lib/theme'
import { useStore } from '../store/context'

const openLink = (uri: string) => {
  if (/^https?:/i.test(uri)) window.codync.app.openExternal(uri)
}

/** The app's own colors: no ground of its own (the page shows through), the ink as text and cursor. */
function appTheme() {
  const css = getComputedStyle(document.documentElement)
  const ink = css.getPropertyValue('--text').trim()
  return { background: 'rgba(0, 0, 0, 0)', foreground: ink, cursor: ink, cursorAccent: css.getPropertyValue('--background').trim(), selectionBackground: `rgba(${css.getPropertyValue('--text-rgb').trim()}, 0.2)` }
}

/** A terminal on the computer, drawn in `surface`: `open` starts it (or finds the one running) and answers its id. */
export interface HostTerminalOptions {
  open: (client: HostClient, cols: number, rows: number) => Promise<string>
  /** Leaving ends it (a setup step); otherwise it keeps running and the next open shows it again. */
  closeOnLeave: boolean
  /** It ended on its own, with `code`. */
  onExit?: () => void
}

/**
 * Draws a terminal of the computer (`/term/<id>`, typed with `termInput`, sized with `termResize`) in `surface`, for as
 * long as `key` stays the same: what it printed so far, then live; a dropped connection reattaches. Answers how it
 * ended (its exit code) or why it couldn't start.
 */
export function useHostTerminal(surface: RefObject<HTMLDivElement | null>, key: string, { open, closeOnLeave, onExit }: HostTerminalOptions) {
  const store = useStore()
  const dark = useDark()
  const drawn = useRef<Terminal | null>(null)
  const [exitCode, setExitCode] = useState<number | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    const el = surface.current
    if (!el) return
    setExitCode(null)
    setError(null)
    const css = getComputedStyle(document.documentElement)
    const scale = parseFloat(css.getPropertyValue('--scale')) || 1
    const term = new Terminal({
      fontFamily: css.getPropertyValue('--mono').trim() || 'monospace',
      fontSize: 12 * scale,
      cursorBlink: true,
      allowTransparency: true,
      theme: appTheme(),
      linkHandler: { activate: (_e, uri) => openLink(uri) },
    })
    const fit = new FitAddon()
    term.loadAddon(fit)
    term.loadAddon(new WebLinksAddon((_e, uri) => openLink(uri)))
    term.open(el)
    drawn.current = term
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
      void client.call('termResize', { term: termId, cols: size.cols, rows: size.rows }).catch(() => {})
    }
    const type = (bytes: Uint8Array) => {
      const client = store.client
      if (exited !== null || !client || !termId) return
      const id = termId
      typing = typing.then(() => client.call('termInput', { term: id, data: toBase64(bytes) }).then(() => {}, () => {}))
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
    const attach = (client: HostClient, id: string) =>
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
        const id = await open(client, size.cols, size.rows)
        if (cancelled) {
          if (closeOnLeave) void client.call('termClose', { term: id }).catch(() => {})
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
        if (!cancelled) setError(e instanceof HostError || e instanceof Error ? e.message : String(e))
        return
      }
      if (!cancelled) onExit?.()
    })()

    return () => {
      cancelled = true
      observer.disconnect()
      subs.forEach((s) => s.dispose())
      stopStream?.()
      const client = store.client
      if (closeOnLeave && exited === null && client && termId) void client.call('termClose', { term: termId }).catch(() => {})
      drawn.current = null
      term.dispose()
    }
    // `open` and `onExit` belong to `key`: a new one comes with a new key.
  }, [store, key])

  // The system's light or dark turns the terminal with the rest of the app.
  useEffect(() => {
    if (drawn.current) drawn.current.options.theme = appTheme()
  }, [dark])

  return { exitCode, error }
}
