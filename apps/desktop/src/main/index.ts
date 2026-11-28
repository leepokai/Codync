import { identity } from './environment'
import { registerAppearance } from './appearance'
import { execFileSync } from 'node:child_process'
import { hostname } from 'node:os'
import { join } from 'node:path'
import { app, autoUpdater, BrowserWindow, clipboard, dialog, ipcMain, nativeTheme, session, shell, type IpcMainInvokeEvent, type OpenDialogOptions } from 'electron'
import type { HostSnapshot, TraySummary, WindowCommand } from '../shared/ipc'
import { chatWindowMinHeight, chatWindowMinWidth } from '../shared/chat-layout'
import { devPort, HostController } from './host-controller'
import { registerFileSaves } from './file-saves'
import { keepArtifactsInFrame, registerArtifacts, registerArtifactScheme } from './artifacts'
import { registerRemoteKeyboard } from './remote-keyboard'
import { registerHostProxy } from './host-proxy'
import { readClipboardFiles, readFiles } from './files'
import { Tray } from './tray'
import { applyDockVisibility, setShowInDock, showInDock } from './dock'
import { AccountService } from './account'
import { Updates, yieldsToInstall } from './updates'
import { handleURL, registerAuthIPC, registerSchemes, urlFromArgv } from './auth'
import { registerSpeech } from './speech'
import { VoiceShortcutKeys } from './voice-shortcut'
import { registerScreenIPC, setScreenAgent } from './screen'
import { registerCloud } from './cloud'
import { registerCloudAccess } from './cloud-access'
import { registerChannels } from './channel'
import { watchAttention } from './attention'
import { registerSSH } from './ssh'
import { registerSSHConfig } from './ssh-config'
import { QuitLifecycle } from './quit-lifecycle'
import { Island } from './island'
import { registerNotifications } from './notifications'
import { registerErrorReports } from './error-reports'
import { registerRendererRecovery } from './renderer-recovery'
import { trackBody } from '../shared/analytics'
import { reportHostSetupFailure } from './analytics'
import { launchAtLogin, openedAtLogin, setLaunchAtLogin, updateLoginItem } from './login-item'
import { applyMenu } from './app-menu'
import { activate, present, webPreferences } from './headless'
import { resolvedLanguage } from '../renderer/lib/l10n'

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
if (!app.isPackaged && process.env.CODYNC_REMOTE_DEBUG) {
  app.commandLine.appendSwitch('remote-debugging-port', process.env.CODYNC_REMOTE_DEBUG)
  // As Playwright does for the Chromium it launches. Reloading the dev server's unbundled modules holds about 750 MB of
  // shared memory until a garbage collection; past a Linux VM's /dev/shm (half its memory) requests fail and the window stays blank.
  app.commandLine.appendSwitch('disable-dev-shm-usage')
}

const host = new HostController()
const reportError = registerErrorReports(host)
registerRendererRecovery(host)
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
    present(chat)
    return chat
  }
  chat = new BrowserWindow({
    width: 1100,
    height: 760,
    minWidth: chatWindowMinWidth,
    minHeight: chatWindowMinHeight,
    title: app.getName(),
    show: false,
    backgroundColor: background(),
    titleBarStyle: isMac ? 'hidden' : 'default',
    webPreferences: webPreferences(join(__dirname, '../preload/index.js')),
  })
  // CODYNC_SHOW_INACTIVE: development and UI checks open the window without taking focus.
  chat.on('ready-to-show', () => chat && present(chat, Boolean(process.env.CODYNC_SHOW_INACTIVE)))
  // The minimum is the page's (chat-window.css): on Windows and Linux the frame and menu bar come on
  // top of it, or they cut the composer off (#476). Their size is known once the window is shown.
  // Not `useContentSize`: on Windows its minimum leaves the menu bar out.
  chat.once('show', () => {
    if (!chat) return
    const [width, height] = chat.getSize()
    const [contentWidth, contentHeight] = chat.getContentSize()
    chat.setMinimumSize(chatWindowMinWidth + width - contentWidth, chatWindowMinHeight + height - contentHeight)
  })
  // Closing hides: the window keeps the stores (and the menu bar's data) alive.
  chat.on('close', (e) => quitLifecycle.closeToBackground(e, () => chat?.hide()))
  chat.on('closed', () => (chat = null))
  watchAttention(chat)
  keepArtifactsInFrame(chat.webContents)
  void chat.loadURL(rendererURL('chat'))
  return chat
}

function openPairing() {
  if (pairing) {
    pairing.webContents.send('app:command', { kind: 'reviewApprovals' } satisfies WindowCommand)
    present(pairing)
    return
  }
  pairing = new BrowserWindow({
    width: 340,
    height: 420,
    useContentSize: true,
    resizable: false,
    title: 'Pair phone',
    show: false,
    backgroundColor: background(),
    webPreferences: webPreferences(join(__dirname, '../preload/index.js')),
  })
  pairing.once('ready-to-show', () => pairing && present(pairing))
  pairing.on('closed', () => (pairing = null))
  void pairing.loadURL(rendererURL('pairing'))
}

export function send(command: WindowCommand, show = true) {
  const window = show ? openChat() : chat
  if (show) activate()
  window?.webContents.send('app:command', command)
}

function registerIPC(tray: Tray) {
  registerHostProxy()
  ipcMain.on('app:hostPort', (e) => (e.returnValue = identity.port))
  ipcMain.on('app:name', (e) => (e.returnValue = app.getName()))
  ipcMain.on('app:scheme', (e) => (e.returnValue = identity.scheme))
  registerFileSaves()
  registerArtifacts()
  registerRemoteKeyboard()
  ipcMain.on('app:version', (e) => (e.returnValue = app.getVersion()))
  ipcMain.on('app:computerName', (e) => (e.returnValue = computerName()))
  ipcMain.on('app:openedAtLogin', (e) => (e.returnValue = openedAtLogin()))
  // `reset` is answered once per start, so a reload or another window doesn't bring the welcome flow back mid-session.
  let onboardingReset = false
  ipcMain.on('app:devOnboarding', (e) => {
    const mode = process.env.CODYNC_DEV_ONBOARDING
    const dev = !app.isPackaged && identity.environment === 'dev'
    e.returnValue = dev && mode === 'skip' ? 'skip' : dev && mode === 'reset' && !onboardingReset ? ((onboardingReset = true), 'reset') : null
  })
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
    const result = await openDialog(e, { properties: ['openFile', 'multiSelections'] })
    return result.canceled ? [] : readFiles(result.filePaths)
  })
  ipcMain.handle('app:pickExport', async (e) => {
    const result = await openDialog(e, { defaultPath: app.getPath('downloads'), properties: ['openFile'], filters: [{ name: 'Bot export', extensions: ['zip'] }] })
    return result.canceled ? null : (result.filePaths[0] ?? null)
  })
  ipcMain.handle('app:clipboardFiles', () => readClipboardFiles())
  ipcMain.on('app:traySummary', (_e, summary: TraySummary) => {
    tray.update(summary)
    applyMenu(summary.language, send)
  })
  ipcMain.on('app:showWindow', () => {
    openChat()
    activate()
  })
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
    // Codync Screen is a login item of its own, which deleting the data leaves behind.
    setScreenAgent(false)
    const { rm } = await import('node:fs/promises')
    const { dataDir } = await import('./host-controller')
    await rm(dataDir, { recursive: true, force: true })
    await rm(app.getPath('userData'), { recursive: true, force: true })
    app.relaunch()
    quitLifecycle.beginQuit()
    app.exit(0)
  })
}

/** An open panel on the window that asked (a sheet on macOS), or a free one when it has none. */
function openDialog(e: IpcMainInvokeEvent, options: OpenDialogOptions) {
  const window = BrowserWindow.fromWebContents(e.sender)
  return window ? dialog.showOpenDialog(window, options) : dialog.showOpenDialog(options)
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
    updates.on('upToDate', () => send({ kind: 'upToDate' }))
    registerIPC(tray)
    island.register()
    registerNotifications(send, reportError)
    // Voice input goes to the island while it's on, else to the chat window's composer.
    new VoiceShortcutKeys((key) => (island.enabled ? island.voiceKey(key) : send({ kind: 'voiceKey', key }, key === 'double'))).register()
    registerAuthIPC()
    registerSpeech()
    registerScreenIPC()
    registerCloud(account)
    registerCloudAccess(account)
    registerChannels(account)
    // Best effort, like `window_recovery`: the host may be down or restarting.
    registerSSH(() => BrowserWindow.getAllWindows(), () => void host.call('track', trackBody({ event: 'ssh_step_after_disconnect' })).catch(() => {}))
    registerSSHConfig()
    // Calls use the microphone (realtime voice); nothing else asks for permissions.
    session.defaultSession.setPermissionRequestHandler((_wc, permission, done) => done(permission === 'media' || permission === 'clipboard-sanitized-write'))
    account.register(() => BrowserWindow.getAllWindows())
    updates.register(() => BrowserWindow.getAllWindows())
    applyMenu(resolvedLanguage('system', app.getLocale()), send)
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
