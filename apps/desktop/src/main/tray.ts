import { join } from 'node:path'
import { app, clipboard, Menu, nativeImage, shell, Tray as ElectronTray } from 'electron'
import type { TraySummary, WindowCommand } from '../shared/ipc'
import type { HostController } from './host-controller'
import type { Updates } from './updates'
import { setShowInDock, showInDock } from './dock'
import { identity } from './environment'

interface Deps {
  openChat: () => unknown
  openPairing: () => void
  send: (command: WindowCommand, show?: boolean) => void
  host: HostController
  updates: Updates
}

const resources = () => (app.isPackaged ? process.resourcesPath : join(__dirname, '../../resources'))
const TEXT_SIZES = [11, 12, 13, 14, 15, 16, 18]

/** The menu bar icon and its system menu: macOS owns layout, selection and submenus. */
export class Tray {
  private tray: ElectronTray
  private summary: TraySummary | null = null
  private launchAtLogin = app.getLoginItemSettings().openAtLogin

  constructor(private deps: Deps) {
    this.tray = new ElectronTray(this.icon(false))
    this.tray.setToolTip(app.getName())
    // Windows opens a tray icon's menu on right-click; a left-click opens the chat.
    if (process.platform === 'win32') this.tray.on('click', () => this.deps.openChat())
    this.rebuild()
  }

  private icon(alert: boolean) {
    // Black template glyphs vanish on Windows' dark taskbar: the app icon reads on either theme.
    if (process.platform === 'win32') return nativeImage.createFromPath(join(resources(), 'icon.png')).resize({ width: 16, height: 16 })
    const image = nativeImage.createFromPath(join(resources(), 'tray', alert ? 'trayAlertTemplate.png' : 'trayTemplate.png'))
    image.setTemplateImage(true)
    return image
  }

  update(summary: TraySummary) {
    const attention = summary.needsAttention !== this.summary?.needsAttention
    this.summary = summary
    if (attention) this.tray.setImage(this.icon(summary.needsAttention))
    this.rebuild()
  }

  hostChanged() {
    this.rebuild()
  }

  /** The renderer draws menu images at 2× (`tray-summary.ts`): shown at half their pixel size. */
  private image(dataURL: string | null) {
    if (!dataURL) return undefined
    const image = nativeImage.createEmpty()
    image.addRepresentation({ scaleFactor: 2, dataURL })
    return image.isEmpty() ? undefined : image
  }

  private rebuild() {
    const { host, send } = this.deps
    const s = this.summary
    const state = host.state
    const items: Electron.MenuItemConstructorOptions[] = []
    const status =
      state.kind === 'missingBinary' ? 'Host not found'
      : state.kind === 'notInstalled' ? 'Host not installed'
      : state.kind === 'starting' ? 'Connecting…'
      : state.kind === 'failed' ? 'Host problem'
      : (s?.status ?? 'Connected')
    items.push({ label: status, enabled: false })
    items.push({ label: `Open ${app.getName()}`, accelerator: 'CmdOrCtrl+O', click: () => this.deps.openChat() })
    if (state.kind === 'running') {
      items.push({ label: 'Pair iPhone…', enabled: s?.canPair ?? false, click: () => this.deps.openPairing() })
    }
    items.push({ type: 'separator' })

    switch (state.kind) {
      case 'missingBinary':
        if (identity.environment === 'dev') {
          items.push({ label: 'Rebuild Codync Dev with its development host.', enabled: false })
          break
        }
        items.push({ label: 'Reinstall Codync or install the host with Homebrew.', enabled: false })
        items.push({ label: 'Copy host install command', click: () => clipboard.writeText('brew install leepokai/codync/codync-host') })
        break
      case 'notInstalled':
        items.push({ label: 'Install host', click: () => void host.install() })
        break
      case 'starting':
        items.push({ label: 'Starting the host…', enabled: false })
        break
      case 'failed':
        items.push({ label: state.message, enabled: false })
        items.push({ label: 'Restart host', click: () => host.restart() })
        items.push({ label: 'Open log', click: () => void shell.openPath(host.logPath) })
        break
      case 'running':
        if (s?.approval) {
          items.push({ label: `Review access request from ${s.approval}…`, click: () => send({ kind: 'reviewApprovals' }) })
          items.push({ type: 'separator' })
        }
        if (!s || s.bots.length === 0) {
          items.push({ label: 'No bots yet', enabled: false })
        } else {
          for (const bot of s.bots) {
            const submenu: Electron.MenuItemConstructorOptions[] = [
              { label: bot.detail, enabled: false },
              { label: 'Open conversation', click: () => send({ kind: 'openBot', ref: bot.ref }) },
            ]
            if (bot.working) submenu.push({ label: 'Stop task', click: () => send({ kind: 'stopBot', ref: bot.ref }, false) })
            items.push({ label: bot.name, icon: this.image(bot.avatar), submenu })
          }
        }
        if (s?.screen) {
          items.push({ type: 'separator' })
          items.push({ label: 'Remote screen', submenu: this.screenMenu(s.screen) })
        }
        if (s && s.usage.length) {
          items.push({ type: 'separator' })
          for (const provider of s.usage) {
            // Buttons, not text: the menu dims a disabled item's image. They open the app.
            items.push({ label: provider.name, icon: this.image(provider.icon), click: () => this.deps.openChat() })
            for (const line of provider.lines) {
              items.push({ label: line.title, icon: this.image(line.bar), click: () => this.deps.openChat() })
            }
          }
        }
        break
    }

    items.push({ type: 'separator' })
    items.push({ label: 'Settings', submenu: this.settingsMenu() })
    if (s?.version) items.push({ label: `Version ${s.version}`, enabled: false })
    items.push({ label: `Quit ${app.getName()}`, accelerator: 'CmdOrCtrl+Q', click: () => app.quit() })
    this.tray.setContextMenu(Menu.buildFromTemplate(items))
  }

  private screenMenu(screen: NonNullable<TraySummary['screen']>): Electron.MenuItemConstructorOptions[] {
    const items: Electron.MenuItemConstructorOptions[] = [
      {
        label: 'Enable remote screen',
        type: 'checkbox',
        checked: screen.enabled,
        click: () => this.deps.send({ kind: 'setRemoteScreen', on: !screen.enabled }, false),
      },
      { label: screen.subtitle, enabled: false },
    ]
    items.push({ label: 'Set up computer access…', click: () => this.deps.send({ kind: 'computerAccess' }, true) })
    if (screen.error) items.push({ label: screen.error, enabled: false })
    return items
  }

  private updatesMenu(): Electron.MenuItemConstructorOptions[] {
    const { updates } = this.deps
    const u = updates.state
    const items: Electron.MenuItemConstructorOptions[] = [
      { label: u.availableVersion ? `Update to ${u.availableVersion}…` : 'Check for Updates…', enabled: u.canCheck || u.staged, click: () => updates.check() },
    ]
    // The release waits until paired iPhones can get the app it needs.
    if (u.waitingForApp) items.push({ label: `Waiting for iPhone app ${u.waitingForApp} to pass App Store review`, enabled: false })
    items.push(
      { label: 'Automatically check for updates', type: 'checkbox', checked: u.autoCheck, enabled: u.supported, click: () => updates.emit('setAutoCheck', !u.autoCheck) },
      { label: 'Automatically download and install', type: 'checkbox', checked: u.autoDownload, enabled: u.supported, click: () => updates.emit('setAutoDownload', !u.autoDownload) },
    )
    if (!u.supported) items.push({ label: 'Updates are available in release builds.', enabled: false })
    if (u.lastCheck) items.push({ label: `Last checked: ${new Date(u.lastCheck).toLocaleString()}`, enabled: false })
    if (u.error) {
      items.push({ label: u.error, enabled: false })
      items.push({ label: 'Retry installing update', click: () => updates.check() })
    }
    return items
  }

  private settingsMenu(): Electron.MenuItemConstructorOptions[] {
    const { host, send } = this.deps
    const s = this.summary
    const style = s?.usageIconStyle ?? 'character'
    const size = s?.textSize ?? 12
    return [
      {
        label: 'Usage icons',
        submenu: [
          { label: 'Character', type: 'radio', checked: style === 'character', click: () => send({ kind: 'setUsageIconStyle', style: 'character' }, false) },
          { label: 'Original', type: 'radio', checked: style === 'original', click: () => send({ kind: 'setUsageIconStyle', style: 'original' }, false) },
        ],
      },
      {
        label: 'Text Size',
        submenu: TEXT_SIZES.map((pt) => ({
          label: pt === 12 ? `${pt} pt (Default)` : `${pt} pt`,
          type: 'radio' as const,
          checked: pt === size,
          click: () => send({ kind: 'setTextSize', size: pt }, false),
        })),
      },
      {
        label: 'Open at login',
        type: 'checkbox',
        checked: this.launchAtLogin,
        click: () => {
          app.setLoginItemSettings({ openAtLogin: !this.launchAtLogin })
          this.launchAtLogin = app.getLoginItemSettings().openAtLogin
          this.rebuild()
        },
      },
      ...(process.platform === 'darwin' ? [{ label: 'Show in Dock', type: 'checkbox' as const, checked: showInDock(), click: () => { setShowInDock(!showInDock()); this.rebuild() } }] : []),
      { type: 'separator' },
      { label: 'Updates', submenu: this.updatesMenu() },
      { type: 'separator' },
      { label: 'Restart host', click: () => host.restart() },
      { label: 'Open log', click: () => void shell.openPath(host.logPath) },
      { type: 'separator' },
      { label: 'Uninstall host service', click: () => void host.uninstall() },
      { type: 'separator' },
      { label: 'Reset all data…', click: () => send({ kind: 'confirmReset' }) },
    ]
  }
}
