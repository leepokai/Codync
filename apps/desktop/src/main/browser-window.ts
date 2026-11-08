import { BrowserWindow, ipcMain, nativeTheme } from 'electron'
import { join } from 'node:path'

/** The Browser panel: one window that shows the host's browser tabs while it is open. */
let window: BrowserWindow | null = null

/**
 * Registers the panel window. It is opened from the chat window; the page itself calls the host through the
 * preload bridge, so this module only owns the window.
 */
export function registerBrowser(rendererURL: (page: string) => string, chatWindow: () => BrowserWindow | null) {
  const chatLocation = new URL(rendererURL('chat')).href
  const open = () => {
    if (window && !window.isDestroyed()) {
      window.show()
      window.focus()
      return
    }
    window = new BrowserWindow({
      width: 1120,
      height: 820,
      minWidth: 640,
      minHeight: 420,
      title: 'Browser',
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
    })
    window.webContents.setIgnoreMenuShortcuts(true)
    void window.loadURL(rendererURL('browser'))
  }
  ipcMain.handle('browser:open', (event) => {
    const chat = chatWindow()
    if (!chat || chat.isDestroyed() || event.sender !== chat.webContents ||
        event.senderFrame !== chat.webContents.mainFrame || event.senderFrame.url !== chatLocation) {
      throw new Error('This window cannot open the browser panel.')
    }
    open()
  })
}
