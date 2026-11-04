import { BrowserWindow, ipcMain, nativeTheme, session, shell } from 'electron'
import { join } from 'node:path'
import type { N8nState } from '../shared/models'
import type { HostController } from './host-controller'

/**
 * The n8n window: one BrowserWindow that shows this window's start page while the
 * host brings the n8n instance up, then the editor itself. The instance belongs to the
 * host, so this module only decides what the window displays.
 */
let window: BrowserWindow | null = null
/** Set once the editor is loaded: navigation stays inside the instance. */
let editorOrigin: string | null = null

export function registerN8n(host: HostController, rendererURL: (page: string) => string, chatWindow: () => BrowserWindow | null, openSettings: () => void) {
  const chatLocation = new URL(rendererURL('chat'))
  const pageLocation = new URL(rendererURL('n8n'))

  /** The host's view of the instance; null while this computer's host isn't answering. */
  async function status(): Promise<N8nState | null> {
    if (!host.local) return null
    try {
      return await host.call<N8nState>('n8nStatus')
    } catch {
      return null
    }
  }

  function allowed(destination: string): boolean {
    if (editorOrigin) {
      try {
        return new URL(destination).origin === editorOrigin
      } catch {
        return false
      }
    }
    return destination === pageLocation.href
  }

  /**
   * The instance's own session cookie logs the editor in, so the editor opens on the
   * canvas instead of the owner's sign-in. The cookie belongs to the instance's origin.
   */
  async function login(url: string, cookie: string | null) {
    if (!cookie) return
    await session.defaultSession.cookies.set({ url, name: 'automan-auth', value: cookie, httpOnly: true, path: '/' })
  }

  function createWindow() {
    window = new BrowserWindow({
      width: 1280,
      height: 820,
      minWidth: 720,
      minHeight: 480,
      title: 'n8n',
      show: false,
      backgroundColor: nativeTheme.shouldUseDarkColors ? '#0A0A0A' : '#FFFFFF',
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'), sandbox: true, contextIsolation: true,
        nodeIntegration: false, webSecurity: true,
      },
    })
    window.on('ready-to-show', () => window?.show())
    window.on('closed', () => {
      window = null
      editorOrigin = null
    })
    window.webContents.setIgnoreMenuShortcuts(true)
    // Links inside the editor (its docs, its sign-in) go to the system browser.
    window.webContents.setWindowOpenHandler(({ url }) => {
      if (/^https?:/i.test(url)) void shell.openExternal(url)
      return { action: 'deny' }
    })
    window.webContents.on('will-navigate', (event, destination) => {
      if (!allowed(destination)) event.preventDefault()
    })
    window.webContents.on('will-redirect', (event, destination) => {
      if (!allowed(destination)) event.preventDefault()
    })
    // The taskbar keeps "n8n" while the editor sets its own document title.
    window.webContents.on('page-title-updated', (event) => event.preventDefault())
    return window
  }

  async function open() {
    if (window && !window.isDestroyed()) {
      window.show()
      window.focus()
      return
    }
    const win = createWindow()
    const s = await status()
    if (win.isDestroyed()) return
    if (s?.state === 'running' && s.url) {
      editorOrigin = new URL(s.url).origin
      await login(s.url, s.auth)
      await win.loadURL(s.url)
    } else {
      editorOrigin = null
      await win.loadURL(pageLocation.href)
    }
  }

  ipcMain.handle('n8n:open', (event) => {
    const chat = chatWindow()
    if (!chat || chat.isDestroyed() || event.sender !== chat.webContents ||
        event.senderFrame !== chat.webContents.mainFrame || new URL(event.senderFrame.url).href !== chatLocation.href) {
      throw new Error('This window cannot open n8n.')
    }
    return open()
  })

  ipcMain.on('n8n:openSettings', (event) => {
    const win = window
    if (!win || win.isDestroyed() || event.sender !== win.webContents || event.senderFrame !== win.webContents.mainFrame) {
      return
    }
    openSettings()
  })

  // The start page calls it once the host reports the instance running.
  ipcMain.handle('n8n:gotoEditor', async (event) => {
    const win = window
    if (!win || win.isDestroyed() || event.sender !== win.webContents ||
        event.senderFrame !== win.webContents.mainFrame || editorOrigin !== null ||
        new URL(event.senderFrame.url).href !== pageLocation.href) {
      throw new Error('This window cannot load the n8n editor.')
    }
    const s = await status()
    if (!s || s.state !== 'running' || !s.url) throw new Error('The n8n instance is not running yet.')
    if (win.isDestroyed()) return
    editorOrigin = new URL(s.url).origin
    await login(s.url, s.auth)
    await win.loadURL(s.url)
  })
}
