import { BrowserWindow, ipcMain, nativeTheme, shell } from 'electron'
import { mkdirSync } from 'node:fs'
import { join } from 'node:path'
import { TOOLS, type ToolId, type ToolState } from '../shared/tools'
import type { HostController } from './host-controller'

/**
 * The windows of the bundled tools: one BrowserWindow per tool that shows its start page while the
 * host brings the tool up, then the tool's own page, served on this computer. The tool belongs to
 * the host, so this module only decides what the window displays. A tool with no sign-in (ComfyUI)
 * needs no session: the window goes straight to its address once the host reports it running.
 */
interface Open {
  window: BrowserWindow
  /** Set once the tool's page is loaded: navigation then stays inside its origin. */
  origin: string | null
}
const windows = new Map<ToolId, Open>()

export function registerToolWindows(host: HostController, rendererURL: (page: string) => string, chatWindow: () => BrowserWindow | null) {
  const chatLocation = new URL(rendererURL('chat'))

  /** The host's view of one tool; null while this computer's host isn't answering. */
  async function status(id: ToolId): Promise<ToolState | null> {
    if (!host.local) return null
    try {
      return await host.call<ToolState>('toolStatus', { id })
    } catch {
      return null
    }
  }

  const known = (id: unknown): id is ToolId => typeof id === 'string' && id in TOOLS

  function allowed(id: ToolId, destination: string): boolean {
    const open = windows.get(id)
    if (open?.origin) {
      try {
        return new URL(destination).origin === open.origin
      } catch {
        return false
      }
    }
    return destination === new URL(rendererURL(TOOLS[id].page)).href
  }

  function createWindow(id: ToolId) {
    const window = new BrowserWindow({
      width: 1280,
      height: 820,
      minWidth: 720,
      minHeight: 480,
      title: TOOLS[id].title,
      show: false,
      backgroundColor: nativeTheme.shouldUseDarkColors ? '#0A0A0A' : '#FFFFFF',
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'), sandbox: true, contextIsolation: true,
        nodeIntegration: false, webSecurity: true,
      },
    })
    windows.set(id, { window, origin: null })
    window.on('ready-to-show', () => window.show())
    window.on('closed', () => windows.delete(id))
    window.webContents.setIgnoreMenuShortcuts(true)
    // Links inside the tool (its docs) go to the system browser.
    window.webContents.setWindowOpenHandler(({ url }) => {
      if (/^https?:/i.test(url)) void shell.openExternal(url)
      return { action: 'deny' }
    })
    window.webContents.on('will-navigate', (event, destination) => {
      if (!allowed(id, destination)) event.preventDefault()
    })
    window.webContents.on('will-redirect', (event, destination) => {
      if (!allowed(id, destination)) event.preventDefault()
    })
    // The taskbar keeps the tool's name while its page sets its own title.
    window.webContents.on('page-title-updated', (event) => event.preventDefault())
    return window
  }

  /** Loads the tool's own page when it runs; false while it doesn't. */
  async function toEditor(id: ToolId): Promise<boolean> {
    const open = windows.get(id)
    const s = await status(id)
    if (!open || open.window.isDestroyed() || s?.state !== 'running' || !s.url) return false
    open.origin = new URL(s.url).origin
    await open.window.loadURL(s.url)
    return true
  }

  async function open(id: ToolId) {
    const existing = windows.get(id)
    if (existing && !existing.window.isDestroyed()) {
      existing.window.show()
      existing.window.focus()
      if (existing.origin === null) await toEditor(id)
      return
    }
    const window = createWindow(id)
    if (!(await toEditor(id)) && !window.isDestroyed()) await window.loadURL(rendererURL(TOOLS[id].page))
  }

  /** Only the chat window (its Extensions card, its rail) or the tool's own start page may ask. */
  function fromChatOrPage(event: Electron.IpcMainInvokeEvent, id: ToolId): boolean {
    const chat = chatWindow()
    const own = windows.get(id)?.window
    const frame = event.senderFrame
    if (!frame) return false
    const isChat = !!chat && !chat.isDestroyed() && event.sender === chat.webContents &&
      frame === chat.webContents.mainFrame && new URL(frame.url).href === chatLocation.href
    const isPage = !!own && !own.isDestroyed() && event.sender === own.webContents &&
      frame === own.webContents.mainFrame && new URL(frame.url).href === new URL(rendererURL(TOOLS[id].page)).href
    return isChat || isPage
  }

  ipcMain.handle('tools:open', (event, id: unknown) => {
    if (!known(id) || !fromChatOrPage(event, id)) throw new Error('This window cannot open that tool.')
    return open(id)
  })

  // The start page calls it once the host reports the tool running.
  ipcMain.handle('tools:gotoEditor', async (event, id: unknown) => {
    const own = known(id) ? windows.get(id) : undefined
    if (!known(id) || !own || own.origin !== null || !fromChatOrPage(event, id) || event.sender !== own.window.webContents) {
      throw new Error('This window cannot load that tool.')
    }
    if (!(await toEditor(id))) throw new Error(`${TOOLS[id].title} is not running yet.`)
  })

  ipcMain.handle('tools:openLog', async (event, id: unknown) => {
    if (!known(id) || !fromChatOrPage(event, id)) throw new Error('This window cannot open that log.')
    const s = await status(id)
    if (s?.log) await shell.openPath(s.log)
  })

  ipcMain.handle('tools:openModels', async (event, id: unknown) => {
    if (!known(id) || !fromChatOrPage(event, id)) throw new Error('This window cannot open that folder.')
    const folder = (await status(id))?.comfy?.modelsDir
    if (!folder) return
    // Before ComfyUI's first start the folder doesn't exist yet; the user is about to put a model in it.
    mkdirSync(folder, { recursive: true })
    await shell.openPath(folder)
  })
}
