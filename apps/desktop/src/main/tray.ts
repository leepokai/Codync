import { join } from 'node:path'
import { app, clipboard, Menu, nativeImage, shell, Tray as ElectronTray } from 'electron'
import type { TrayImage, TraySummary, TraySymbol, WindowCommand } from '../shared/ipc'
import type { HostController } from './host-controller'
import type { Updates } from './updates'
import { updateAction } from '../shared/update-action'
import { setShowInDock, showInDock } from './dock'
import { identity } from './environment'
import { islandSupported, type Island } from './island'
import { islandLabel } from '../shared/island'
import { resolvedLanguage, translate } from '../renderer/lib/l10n'
import { launchAtLogin, setLaunchAtLogin } from './login-item'

interface Deps {
  openChat: () => unknown
  openPairing: () => void
  send: (command: WindowCommand, show?: boolean) => void
  host: HostController
  updates: Updates
  island: Island
}

const resources = () => (app.isPackaged ? process.resourcesPath : join(__dirname, '../../resources'))
const TEXT_SIZES = [11, 12, 13, 14, 15, 16, 18]

/** The menu bar icon and its system menu: macOS owns layout, selection and submenus. */
export class Tray {
  private tray: ElectronTray
  private summary: TraySummary | null = null
  private launchAtLogin = launchAtLogin()

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

  /** Menu-bar labels are a system menu, so they translate here. Slots are filled after lookup. */
  private tr(text: string) {
    const preference = this.summary?.language ?? resolvedLanguage('system', app.getLocale())
    return translate(text, preference, 'en')
  }

  /**
   * The renderer draws menu images at 1× and 2× (`tray-summary.ts`): each screen gets its own pixels.
   * A template image follows the menu's highlight and appearance on macOS, like its SF Symbols.
   */
  private image(drawn: TrayImage | null, template = false) {
    if (!drawn) return undefined
    const image = nativeImage.createEmpty()
    image.addRepresentation({ scaleFactor: 1, dataURL: drawn.x1 })
    image.addRepresentation({ scaleFactor: 2, dataURL: drawn.x2 })
    if (image.isEmpty()) return undefined
    image.setTemplateImage(template)
    return image
  }

  /** The symbol beside an action: SF Symbols on macOS, the renderer's drawing of it on Windows and Linux. */
  private symbol(name: TraySymbol) {
    if (process.platform === 'darwin') return nativeImage.createMenuSymbol(name)
    return this.image(this.summary?.symbols?.[name] ?? null)
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
    items.push({ label: this.tr(status), enabled: false })
    items.push({ label: this.tr('Open {name}').replaceAll('{name}', app.getName()), icon: this.symbol('macwindow'), accelerator: 'CmdOrCtrl+O', click: () => this.deps.openChat() })
    if (state.kind === 'running') {
      items.push({ label: this.tr('Pair iPhone…'), icon: this.symbol('qrcode'), enabled: s?.canPair ?? false, click: () => this.deps.openPairing() })
    }
    items.push({ type: 'separator' })

    switch (state.kind) {
      case 'missingBinary':
        if (identity.environment === 'dev') {
          items.push({ label: this.tr('Rebuild Codync Dev with its development host.'), enabled: false })
          break
        }
        items.push({ label: this.tr('Reinstall Codync or install the host with Homebrew.'), enabled: false })
        items.push({ label: this.tr('Copy host install command'), icon: this.symbol('square.on.square'), click: () => clipboard.writeText('brew install leepokai/codync/codync-host') })
        break
      case 'notInstalled':
        items.push({ label: this.tr('Install host'), icon: this.symbol('arrow.down.circle'), click: () => void host.install() })
        break
      case 'starting':
        items.push({ label: this.tr('Starting the host…'), enabled: false })
        break
      case 'failed':
        items.push({ label: this.tr(state.message), enabled: false })
        items.push({ label: this.tr('Restart host'), icon: this.symbol('arrow.clockwise.circle'), click: () => host.restart() })
        items.push({ label: this.tr('Open log'), icon: this.symbol('list.bullet.rectangle'), click: () => void shell.openPath(host.logPath) })
        break
      case 'running':
        if (s?.approval) {
          items.push({ label: this.tr('Review access request from {name}…').replaceAll('{name}', s.approval), icon: this.symbol('person.crop.circle.badge.checkmark'), click: () => send({ kind: 'reviewApprovals' }) })
          items.push({ type: 'separator' })
        }
        if (!s || s.bots.length === 0) {
          items.push({ label: this.tr('No bots yet'), enabled: false })
        } else {
          for (const bot of s.bots) {
            const submenu: Electron.MenuItemConstructorOptions[] = [
              { label: this.tr(bot.detail), enabled: false },
              { label: this.tr('Open conversation'), click: () => send({ kind: 'openBot', ref: bot.ref }) },
            ]
            if (bot.working) submenu.push({ label: this.tr('Stop task'), click: () => send({ kind: 'stopBot', ref: bot.ref }, false) })
            items.push({ label: bot.name, icon: this.image(bot.avatar), submenu })
          }
        }
        if (s?.screen || s?.networks) items.push({ type: 'separator' })
        if (s?.screen) items.push({ label: this.tr('Remote screen'), icon: this.symbol('display'), submenu: this.screenMenu(s.screen) })
        if (s?.networks) items.push({ label: this.tr('Network'), icon: this.image(s.networkIcon, process.platform === 'darwin'), submenu: this.networkMenu(s.networks) })
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
    // Settings and Quit close the menu as a pair; the version lives in Settings → Updates.
    items.push({ label: this.tr('Settings'), icon: this.symbol('gearshape'), submenu: this.settingsMenu() })
    items.push({ label: this.tr('Quit {name}').replaceAll('{name}', app.getName()), icon: this.symbol('power'), accelerator: 'CmdOrCtrl+Q', click: () => app.quit() })
    this.tray.setContextMenu(Menu.buildFromTemplate(items))
  }

  private screenMenu(screen: NonNullable<TraySummary['screen']>): Electron.MenuItemConstructorOptions[] {
    const items: Electron.MenuItemConstructorOptions[] = [
      {
        label: this.tr('Enable remote screen'),
        type: 'checkbox',
        checked: screen.enabled,
        click: () => this.deps.send({ kind: 'setRemoteScreen', on: !screen.enabled }, false),
      },
      { label: this.tr(screen.subtitle), enabled: false },
    ]
    items.push({ label: this.tr('Set up computer access…'), click: () => this.deps.send({ kind: 'computerAccess' }, true) })
    if (screen.error) items.push({ label: this.tr(screen.error), enabled: false })
    return items
  }

  /** Each network's computers, as last heard; any row opens Settings → Network. */
  private networkMenu(networks: NonNullable<TraySummary['networks']>): Electron.MenuItemConstructorOptions[] {
    const open = () => this.deps.send({ kind: 'networkSettings' })
    const items: Electron.MenuItemConstructorOptions[] = []
    for (const net of networks) {
      if (items.length) items.push({ type: 'separator' })
      items.push({ label: net.title, enabled: false })
      // Buttons, not text: the menu dims a disabled item's image.
      for (const c of net.computers) items.push({ label: c.label, icon: this.image(c.dot), click: open })
    }
    if (!networks.length) items.push({ label: this.tr('Connect your computers so bots on one can message bots on another.'), enabled: false })
    items.push({ type: 'separator' }, { label: this.tr('Network settings…'), click: open })
    return items
  }

  private updatesMenu(): Electron.MenuItemConstructorOptions[] {
    const { updates } = this.deps
    const u = updates.state
    const action = updateAction(u)
    const items: Electron.MenuItemConstructorOptions[] = [
      { label: this.tr(action.label), enabled: action.enabled, click: () => updates.check() },
      { label: this.tr('Automatically check for updates'), type: 'checkbox', checked: u.autoCheck, enabled: u.supported, click: () => updates.emit('setAutoCheck', !u.autoCheck) },
      { label: this.tr('Automatically download and install'), type: 'checkbox', checked: u.autoDownload, enabled: u.supported, click: () => updates.emit('setAutoDownload', !u.autoDownload) },
    ]
    if (!u.supported) items.push({ label: this.tr('Updates are available in release builds.'), enabled: false })
    items.push({ label: this.tr('Version {version}').replaceAll('{version}', app.getVersion()), enabled: false })
    if (u.lastCheck) items.push({ label: this.tr('Last checked {when}').replaceAll('{when}', new Date(u.lastCheck).toLocaleString()), enabled: false })
    if (u.error) {
      items.push({ label: this.tr(u.error), enabled: false })
      items.push({ label: this.tr('Retry installing update'), click: () => updates.check() })
    }
    return items
  }

  private settingsMenu(): Electron.MenuItemConstructorOptions[] {
    const { host, send, island } = this.deps
    const s = this.summary
    const style = s?.usageIconStyle ?? 'character'
    const size = s?.textSize ?? 12
    return [
      {
        label: this.tr('Usage icons'),
        submenu: [
          { label: this.tr('Character'), type: 'radio', checked: style === 'character', click: () => send({ kind: 'setUsageIconStyle', style: 'character' }, false) },
          { label: this.tr('Original'), type: 'radio', checked: style === 'original', click: () => send({ kind: 'setUsageIconStyle', style: 'original' }, false) },
        ],
      },
      {
        label: this.tr('Text Size'),
        submenu: TEXT_SIZES.map((pt) => ({
          label: pt === 12 ? this.tr('{size} pt (Default)').replaceAll('{size}', String(pt)) : `${pt} pt`,
          type: 'radio' as const,
          checked: pt === size,
          click: () => send({ kind: 'setTextSize', size: pt }, false),
        })),
      },
      {
        label: this.tr('Open at login'),
        type: 'checkbox',
        checked: this.launchAtLogin,
        click: () => {
          this.launchAtLogin = setLaunchAtLogin(!this.launchAtLogin)
          this.rebuild()
        },
      },
      ...(process.platform === 'darwin' ? [{ label: this.tr('Show in Dock'), type: 'checkbox' as const, checked: showInDock(), click: () => { setShowInDock(!showInDock()); this.rebuild() } }] : []),
      ...(islandSupported ? [{ label: this.tr(islandLabel(process.platform)), type: 'checkbox' as const, checked: island.enabled, click: () => island.setEnabled(!island.enabled) }] : []),
      { type: 'separator' },
      { label: this.tr('Updates'), submenu: this.updatesMenu() },
      { type: 'separator' },
      { label: this.tr('Restart host'), click: () => host.restart() },
      { label: this.tr('Open log'), click: () => void shell.openPath(host.logPath) },
      { type: 'separator' },
      { label: this.tr('Uninstall host service'), click: () => void host.uninstall() },
      { type: 'separator' },
      { label: this.tr('Reset all data…'), click: () => send({ kind: 'confirmReset' }) },
    ]
  }
}
