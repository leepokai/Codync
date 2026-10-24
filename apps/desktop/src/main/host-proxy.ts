import { ipcMain, type WebContents } from 'electron'
import type { CallError } from '../shared/ipc'
import { noteAnalyticsSetting } from './analytics'

// Loopback HTTP + SSE for the renderer. The host has no CORS (it's a loopback API for local
// apps), so requests leave from the main process.

const streams = new Map<string, AbortController>()

function isLoopback(url: string) {
  try {
    const host = new URL(url).hostname
    return host === '127.0.0.1' || host === 'localhost' || host === '[::1]'
  } catch {
    return false
  }
}

async function call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown> {
  if (!isLoopback(baseURL)) throw { status: 0, message: 'Not a loopback host.' } satisfies CallError
  let res: Response
  try {
    res = await fetch(`${baseURL}/api/${method}`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify(body ?? {}),
      signal: AbortSignal.timeout(timeoutMs),
    })
  } catch {
    throw { status: 0, message: "Can't reach your computer. Is it on and connected to the internet?" } satisfies CallError
  }
  const text = await res.text()
  if (!res.ok) {
    let message = `Host error ${res.status}`
    try {
      message = (JSON.parse(text) as { error?: string }).error ?? message
    } catch {}
    throw { status: res.status, message } satisfies CallError
  }
  const value: unknown = text ? JSON.parse(text) : {}
  // The main process keeps the analytics setting for when there's no host (`host_setup_failed`).
  noteAnalyticsSetting(baseURL, method, value)
  return value
}

function stream(sender: WebContents, id: string, url: string, token: string) {
  const controller = new AbortController()
  streams.set(id, controller)
  const send = (channel: string, payload: unknown) => {
    if (!sender.isDestroyed()) sender.send(channel, id, payload)
  }
  void (async () => {
    // Idle timeout, not a total one: the host pings every 15 s, so silence this long means it's gone.
    let idle: NodeJS.Timeout | null = null
    const arm = () => {
      if (idle) clearTimeout(idle)
      idle = setTimeout(() => controller.abort(), 45_000)
    }
    try {
      if (!isLoopback(url)) throw { status: 0, message: 'Not a loopback host.' }
      arm()
      const res = await fetch(url, {
        headers: { Authorization: `Bearer ${token}`, Accept: 'text/event-stream' },
        signal: controller.signal,
      })
      if (res.status !== 200 || !res.body) throw { status: res.status, message: `Host error ${res.status}` }
      const reader = res.body.getReader()
      const decoder = new TextDecoder()
      let buffer = ''
      for (;;) {
        const { value, done } = await reader.read()
        if (done) break
        arm()
        buffer += decoder.decode(value, { stream: true })
        let nl: number
        while ((nl = buffer.indexOf('\n')) >= 0) {
          const line = buffer.slice(0, nl).replace(/\r$/, '')
          buffer = buffer.slice(nl + 1)
          if (line.startsWith('data:')) send('host:stream:data', line.slice(5).trimStart())
        }
      }
      send('host:stream:end', null)
    } catch (error) {
      const e = error as Partial<CallError>
      send('host:stream:end', controller.signal.aborted && streams.has(id)
        ? { status: 0, message: "Can't reach your computer." }
        : { status: e.status ?? 0, message: e.message ?? "Can't reach your computer." })
    } finally {
      if (idle) clearTimeout(idle)
      streams.delete(id)
    }
  })()
}

export function registerHostProxy() {
  ipcMain.handle('host:call', (_e, baseURL: string, token: string, method: string, body: unknown, timeoutMs: number) =>
    call(baseURL, token, method, body, timeoutMs).then(
      (value) => ({ ok: true, value }),
      (error: CallError) => ({ ok: false, error }),
    ),
  )
  ipcMain.on('host:stream:open', (e, id: string, url: string, token: string) => stream(e.sender, id, url, token))
  ipcMain.on('host:stream:close', (_e, id: string) => {
    const controller = streams.get(id)
    streams.delete(id)
    controller?.abort()
  })
}
