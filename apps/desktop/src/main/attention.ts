import { powerMonitor, type BrowserWindow } from 'electron'

// Whether someone is looking at the chat window: it's shown and not minimized, and the computer is in use
// (unlocked, input in the last 10 minutes). The window keeps its cloud computers connected only then, as a phone
// does in the foreground, since a connected device keeps a cloud computer awake (and billed).

const AWAY_SECONDS = 600
const CHECK_MS = 30_000

let attended = false
const listeners = new Set<(attended: boolean) => void>()

export const isAttended = () => attended

/** Calls `cb` on each change; the returned function stops it. */
export function onAttention(cb: (attended: boolean) => void) {
  listeners.add(cb)
  return () => void listeners.delete(cb)
}

function setAttended(now: boolean) {
  if (now === attended) return
  attended = now
  for (const cb of listeners) cb(now)
}

export function watchAttention(window: BrowserWindow) {
  let last: boolean | null = null
  const check = () => {
    if (window.isDestroyed()) return
    const now = window.isVisible() && !window.isMinimized() && powerMonitor.getSystemIdleState(AWAY_SECONDS) === 'active'
    setAttended(now)
    if (now === last) return
    last = now
    window.webContents.send('app:attended', now)
  }
  // The renderer asks once it listens; until then, events only remember the state.
  window.webContents.ipc.handle('app:attended', () => {
    last = null
    check()
    return last
  })
  for (const event of ['show', 'hide', 'minimize', 'restore'] as const) window.on(event as 'show', check)
  const events = ['lock-screen', 'unlock-screen', 'suspend', 'resume'] as const
  for (const event of events) powerMonitor.on(event as 'suspend', check)
  const timer = setInterval(check, CHECK_MS)
  window.on('closed', () => {
    setAttended(false)
    clearInterval(timer)
    for (const event of events) powerMonitor.off(event as 'suspend', check)
  })
}
