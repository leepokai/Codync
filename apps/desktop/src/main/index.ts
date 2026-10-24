import { identity } from './environment'
import { registerAppearance } from './appearance'
import { execFileSync } from 'node:child_process'
import { hostname } from 'node:os'
import { join } from 'node:path'
import { app, autoUpdater, BrowserWindow, clipboard, dialog, ipcMain, Menu, nativeTheme, session, shell } from 'electron'
import type { HostSnapshot, WindowCommand } from '../shared/ipc'
import { devPort, HostController } from './host-controller'
import { registerFileSaves } from './file-saves'
import { keepArtifactsInFrame, openWebLink, registerArtifacts, registerArtifactScheme } from './artifacts'
import { registerHostProxy } from './host-proxy'
import { readClipboardFiles, readFiles } from './files'
import { Tray } from './tray'
import { applyDockVisibility, setShowInDock, showInDock } from './dock'
import { AccountService } from './account'
import { Updates, yieldsToInstall } from './updates'
import { handleURL, registerAuthIPC, registerSchemes, urlFromArgv } from './auth'
import { registerSpeech } from './speech'
import { VoiceShortcutKeys } from './voice-shortcut'
import { registerScreenIPC } from './screen'
import { registerCloud } from './cloud'
import { registerSSH } from './ssh'
import { QuitLifecycle } from './quit-lifecycle'
import { Island } from './island'
import { registerErrorReports } from './error-reports'
import { reportHostSetupFailure } from './analytics'
import { launchAtLogin, openedAtLogin, setLaunchAtLogin, updateLoginItem } from './login-item'

/** macOS's user-facing computer name, else the host name. */
function computerName() {
  if (process.platform === 'darwin') {
    try {
      return execFileSync('/usr/sbin/scutil', ['--get', 'ComputerName'], { encoding: 'utf8' }).trim()
    } catch {}
  }
  return hostname()
}

// UI checks drive the renderer over the DevTools protocol (development builds only).
if (!app.isPackaged && process.env.CODYNC_REMOTE_DEBUG) app.commandLine.appendSwitch('remote-debugging-port', process.env.CODYNC_REMOTE_DEBUG)

const host = new HostController()
registerErrorReports(host)
host.on('setupFailed', reportHostSetupFailure)
const account = new AccountService()
const updates = new Updates(host)
let chat: BrowserWindow | null = null
let pairing: BrowserWindow | null = null
const quitLifecycle = new QuitLifecycle(app, autoUpdater)
const isMac = process.platform === 'darwin'

function rendererURL(page: string) {
  return process.env.ELECTRON_RENDERER_URL
    ? `${process.env.ELECTRON_RENDERER_URL}/?window=${page}`
    : `file://${join(__dirname, '../renderer/index.html')}?window=${page}`
}

const background = () => (nativeTheme.shouldUseDarkColors ? '#0A0A0A' : '#FFFFFF')

function snapshot(): HostSnapshot {
  return { state: host.state, local: host.local, dev: devPort !== null, logPath: host.logPath }
}

/** The chat window: roster on the left, the conversation on the right (Grok Bot's desktop layout). */
function openChat() {
  if (chat) {
    // Shown again after being hidden (closed to the background): the launch itself is `app_opened`.
    if (!chat.isVisible() && !chat.webContents.isLoading()) chat.webContents.send('app:command', { kind: 'windowOpened' } satisfies WindowCommand)
    chat.show()
    chat.focus()
    return chat
  }
  chat = new BrowserWindow({
    width: 1100,
    height: 760,
    minWidth: 760,
    minHeight: 500,
    title: app.getName(),
    show: false,
    backgroundColor: background(),
    titleBarStyle: isMac ? 'hidden' : 'default',
    webPreferences: { preload: join(__dirname, '../preload/index.js'), sandbox: true, contextIsolation: true },
  })
  // CODYNC_SHOW_INACTIVE: development and UI checks open the window without taking focus.
  chat.on('ready-to-show', () => (process.env.CODYNC_SHOW_INACTIVE ? chat?.showInactive() : chat?.show()))
  // Closing hides: the window keeps the stores (and the menu bar's data) alive.
  chat.on('close', (e) => quitLifecycle.closeToBackground(e, () => chat?.hide()))
  chat.on('closed', () => (chat = null))
  keepArtifactsInFrame(chat.webContents)
  // A new window (an artifact's target=_blank link or window.open, too) opens in the browser instead.
  chat.webContents.setWindowOpenHandler(({ url }) => {
    openWebLink(url)
    return { action: 'deny' }
  })
  void chat.loadURL(rendererURL('chat'))
  return chat
}

function openPairing() {
  if (pairing) {
    pairing.webContents.send('app:command', { kind: 'reviewApprovals' } satisfies WindowCommand)
    pairing.show()
    pairing.focus()
    return
  }
  pairing = new BrowserWindow({
    width: 340,
    height: 420,
    useContentSize: true,
    resizable: false,
    title: 'Pair iPhone',
    backgroundColor: background(),
    webPreferences: { preload: join(__dirname, '../preload/index.js'), sandbox: true, contextIsolation: true },
  })
  pairing.on('closed', () => (pairing = null))
  void pairing.loadURL(rendererURL('pairing'))
}

export function send(command: WindowCommand, show = true) {
  const window = show ? openChat() : chat
  if (show && isMac) app.focus({ steal: true })
  window?.webContents.send('app:command', command)
}

function menu() {
  const template: Electron.MenuItemConstructorOptions[] = [
    ...(isMac ? [{ role: 'appMenu' as const }] : []),
    { role: 'fileMenu' },
    { role: 'editMenu' },
    {
      label: 'View',
      submenu: [
        { label: 'Bigger', accelerator: 'CmdOrCtrl+=', click: () => send({ kind: 'stepTextSize', up: true }, false) },
        { label: 'Smaller', accelerator: 'CmdOrCtrl+-', click: () => send({ kind: 'stepTextSize', up: false }, false) },
        { label: 'Actual Size', accelerator: 'CmdOrCtrl+0', click: () => send({ kind: 'stepTextSize', up: null }, false) },
        { type: 'separator' },
        { label: 'New Bot', accelerator: 'CmdOrCtrl+N', click: () => send({ kind: 'newBot' }) },
        { label: 'Search Bots', accelerator: 'CmdOrCtrl+F', click: () => send({ kind: 'search' }) },
        { label: 'Toggle Sidebar', accelerator: 'Ctrl+CmdOrCtrl+S', click: () => send({ kind: 'toggleSidebar' }, false) },
        { label: 'Conversation Details', accelerator: 'Alt+CmdOrCtrl+I', click: () => send({ kind: 'toggleDetails' }, false) },
        { label: 'Refresh', accelerator: 'CmdOrCtrl+R', click: () => send({ kind: 'refresh' }, false) },
        { type: 'separator' },
        // The dev page reload sits on ⇧⌘R, leaving ⌘R to Refresh.
        ...(app.isPackaged ? [] : [{ role: 'toggleDevTools' as const }, { role: 'forceReload' as const }]),
        { role: 'togglefullscreen' },
      ],
    },
    { role: 'windowMenu' },
  ]
  Menu.setApplicationMenu(Menu.buildFromTemplate(template))
}

function registerIPC(tray: Tray) {
  registerHostProxy()
  ipcMain.on('app:hostPort', (e) => (e.returnValue = identity.port))
  ipcMain.on('app:name', (e) => (e.returnValue = app.getName()))
  ipcMain.on('app:scheme', (e) => (e.returnValue = identity.scheme))
  registerFileSaves()
  registerArtifacts()
  ipcMain.on('app:version', (e) => (e.returnValue = app.getVersion()))
  ipcMain.on('app:computerName', (e) => (e.returnValue = computerName()))
  ipcMain.on('app:openedAtLogin', (e) => (e.returnValue = openedAtLogin()))
  ipcMain.on('app:debugOpen', (e) => (e.returnValue = app.isPackaged ? null : (process.env.CODYNC_DEBUG_OPEN ?? null)))
  ipcMain.handle('host:snapshot', () => snapshot())
  ipcMain.on('host:install', () => void host.install())
  ipcMain.on('host:restart', () => host.restart())
  ipcMain.on('app:openExternal', (_e, url: string) => {
    if (/^(https?|mailto|x-apple\.systempreferences):/.test(url)) void shell.openExternal(url)
  })
  ipcMain.on('app:openSettings', (_e, url: string) => {
    if (url.startsWith('x-apple.systempreferences:')) void shell.openExternal(url)
  })
  ipcMain.on('app:copy', (_e, text: string) => clipboard.writeText(text))
  ipcMain.handle('app:pickFiles', async (e) => {
    const window = BrowserWindow.fromWebContents(e.sender)
    const result = window
      ? await dialog.showOpenDialog(window, { properties: ['openFile', 'multiSelections'] })
      : await dialog.showOpenDialog({ properties: ['openFile', 'multiSelections'] })
    return result.canceled ? [] : readFiles(result.filePaths)
  })
  ipcMain.handle('app:clipboardFiles', () => readClipboardFiles())
  ipcMain.on('app:traySummary', (_e, summary) => tray.update(summary))
  ipcMain.handle('app:showInDock', () => showInDock())
  ipcMain.handle('app:setShowInDock', (_e, on: boolean) => {
    const visible = setShowInDock(on)
    tray.hostChanged()
    return visible
  })
  ipcMain.handle('app:launchAtLogin', () => launchAtLogin())
  ipcMain.handle('app:setLaunchAtLogin', (_e, on: boolean) => setLaunchAtLogin(on))
  ipcMain.handle('app:resetAllData', async () => {
    // Tell paired devices first: connected ones see "No access" at once, and the account drops
    // this computer instead of keeping an unreachable older copy of it.
    await host.call('unclaim', {}, 30_000).catch(() => {})
    const devices = await host.call<{ devices: { key: string }[] }>('devices').catch(() => ({ devices: [] }))
    for (const device of devices.devices) await host.call('revokeDevice', { key: device.key }).catch(() => {})
    await account.signOutAll()
    if (!(await host.uninstall())) throw new Error('The host could not be safely uninstalled. Data was preserved.')
    const { rm } = await import('node:fs/promises')
    const { dataDir } = await import('./host-controller')
    await rm(dataDir, { recursive: true, force: true })
    await rm(app.getPath('userData'), { recursive: true, force: true })
    app.relaunch()
    quitLifecycle.beginQuit()
    app.exit(0)
  })
}

if (yieldsToInstall()) {
  app.exit(0)
} else if (!app.requestSingleInstanceLock()) {
  if (!app.isPackaged) console.error(`${app.getName()} is already running. Quit it first, or run another copy beside it: npm run dev -- --name <name>`)
  app.quit()
} else {
  registerSchemes()
  registerArtifactScheme()
  app.on('second-instance', (_e, argv) => {
    const url = urlFromArgv(argv)
    if (url) handleURL(url)
    else openChat()
  })
  app.on('activate', () => openChat())
  app.on('window-all-closed', () => {
    // A menu bar app: it keeps running with no window.
  })
  void app.whenReady().then(() => {
    registerAppearance()
    // Packaged builds take the icon from the bundle (resources/AppIcon.icon); a development run
    // would otherwise show Electron's.
    if (isMac && !app.isPackaged) app.dock?.setIcon(join(__dirname, '../../resources/icon.png'))
    applyDockVisibility()
    updateLoginItem()
    const island = new Island({ url: rendererURL('island'), preload: join(__dirname, '../preload/index.js'), send, openApp: openChat, changed: () => tray.hostChanged() })
    const tray = new Tray({ openChat, openPairing, send, host, updates, island })
    updates.on('change', () => tray.hostChanged())
    registerIPC(tray)
    island.register()
    // Voice input goes to the island while it's on, else to the chat window's composer.
    new VoiceShortcutKeys((key) => (island.enabled ? island.voiceKey(key) : send({ kind: 'voiceKey', key }, key === 'double'))).register()
    registerAuthIPC()
    registerSpeech()
    registerScreenIPC()
    registerCloud(account)
    registerSSH(() => BrowserWindow.getAllWindows())
    // Calls use the microphone (realtime voice); nothing else asks for permissions.
    session.defaultSession.setPermissionRequestHandler((_wc, permission, done) => done(permission === 'media' || permission === 'clipboard-sanitized-write'))
    account.register(() => BrowserWindow.getAllWindows())
    updates.register(() => BrowserWindow.getAllWindows())
    menu()
    host.on('change', () => {
      const s = snapshot()
      for (const w of BrowserWindow.getAllWindows()) w.webContents.send('host:change', s)
      tray.hostChanged()
    })
    nativeTheme.on('updated', () => {
      // Only the opaque windows: a color on the transparent notch island paints its whole panel.
      for (const w of [chat, pairing]) w?.setBackgroundColor(background())
    })
    host.start()
    openChat()
  })
}
