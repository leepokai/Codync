import { BrowserWindow, ipcMain } from 'electron'
import { call } from './host-proxy'

// The Remote screen window: one per computer, showing that computer's display with the video in
// it (renderer: views/screen/). The window owns the WebRTC connection; this side only opens it,
// keeps its keys away from the app menu, and closes the viewer session on the computer when the
// window goes for any reason (the renderer closes it itself on the normal path).

interface Deps {
  preload: string
  /** The renderer's URL for the computer's viewer. */
  url: (computerId: string) => string
}

/** What the window last reported: the loopback host it talks to and its live session. */
interface Live {
  baseURL: string
  token: string
  session: string | null
}

export class ScreenViewers {
  private windows = new Map<string, BrowserWindow>()
  private live = new Map<number, Live>()

  constructor(private deps: Deps) {}

  register() {
    ipcMain.on('screenViewer:open', (_e, computerId: unknown, name: unknown) => {
      if (typeof computerId === 'string' && computerId && computerId.length < 200) this.open(computerId, typeof name === 'string' ? name.slice(0, 200) : '')
    })
    ipcMain.on('screenViewer:report', (e, baseURL: unknown, token: unknown, session: unknown) => {
      if (typeof baseURL !== 'string' || typeof token !== 'string') return
      this.live.set(e.sender.id, { baseURL, token, session: typeof session === 'string' ? session : null })
    })
    ipcMain.on('screenViewer:close', (e) => BrowserWindow.fromWebContents(e.sender)?.close())
    ipcMain.handle('screenViewer:list', () => this.openComputers())
  }

  /** The computers whose Remote screen window is open. */
  private openComputers(): string[] {
    return [...this.windows].filter(([, window]) => !window.isDestroyed()).map(([computerId]) => computerId)
  }

  /** Tells every window (the details panel's preview yields to the full viewer) which windows are open. */
  private announce() {
    const open = this.openComputers()
    for (const window of BrowserWindow.getAllWindows()) if (!window.isDestroyed()) window.webContents.send('screenViewer:windows', open)
  }

  open(computerId: string, name: string) {
    const existing = this.windows.get(computerId)
    if (existing && !existing.isDestroyed()) {
      if (existing.isMinimized()) existing.restore()
      existing.show()
      existing.focus()
      return
    }
    const window = new BrowserWindow({
      width: 1280,
      height: 800,
      minWidth: 480,
      minHeight: 320,
      title: name || 'Remote screen',
      backgroundColor: '#000000',
      webPreferences: { preload: this.deps.preload, sandbox: true, contextIsolation: true },
    })
    const id = window.webContents.id
    this.windows.set(computerId, window)
    this.announce()
    // Cmd+W, Cmd+Q, Cmd+F and the rest go to the remote computer while this window has focus, not to this app's menu.
    window.webContents.setIgnoreMenuShortcuts(true)
    // Windows and Linux would otherwise open the menu bar on a lone Alt or F10, and the keys after it never reach the remote.
    if (process.platform !== 'darwin') window.removeMenu()
    window.webContents.setWindowOpenHandler(() => ({ action: 'deny' }))
    window.on('closed', () => {
      if (this.windows.get(computerId) === window) this.windows.delete(computerId)
      this.announce()
      const live = this.live.get(id)
      this.live.delete(id)
      if (live?.session) void call(live.baseURL, live.token, 'screenClose', { session: live.session }, 15_000).catch(() => {})
    })
    void window.loadURL(this.deps.url(computerId))
  }
}
