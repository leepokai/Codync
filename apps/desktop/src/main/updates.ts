import { EventEmitter } from 'node:events'
import { app, BrowserWindow, ipcMain, powerMonitor } from 'electron'
import electronUpdater, { type UpdateInfo } from 'electron-updater'
import { isBelow, parseVersion } from '../shared/compat'
import type { UpdateState } from '../shared/ipc'
import { isMainEnvironment } from './account'
import { prefs, type HostController } from './host-controller'

const { autoUpdater } = electronUpdater

/**
 * The app's own updates (electron-updater, GitHub releases; packaged `main` builds only, so a
 * local `dev` build never replaces itself with a release). Codync
 * coordinates its background services before the installer replaces the app: the host is
 * stopped first, and a restart marker makes the next launch reinstall it from the new bundle.
 */
const enabled = app.isPackaged && isMainEnvironment()

export class Updates extends EventEmitter {
  state: UpdateState = {
    supported: enabled,
    canCheck: enabled,
    checking: false,
    availableVersion: null,
    staged: false,
    autoCheck: prefs.get('updatesAutoCheck') !== false,
    autoDownload: prefs.get('updatesAutoDownload') === true,
    lastCheck: (prefs.get('updatesLastCheck') as number | undefined) ?? null,
    error: null,
    waitingForApp: null,
  }
  private userInitiated = false
  private preparing: Promise<boolean> | null = null
  private prepared = false
  private installing = false
  private updateGeneration = 0
  private appStoreVersion: string | null = null

  constructor(private host: HostController) {
    super()
  }

  register(windows: () => BrowserWindow[]) {
    ipcMain.handle('updates:state', () => this.state)
    ipcMain.on('updates:check', () => this.check())
    ipcMain.on('updates:autoCheck', (_e, on: boolean) => this.setAutoCheck(on))
    ipcMain.on('updates:autoDownload', (_e, on: boolean) => this.setAutoDownload(on))
    this.on('setAutoCheck', (on: boolean) => this.setAutoCheck(on))
    this.on('setAutoDownload', (on: boolean) => this.setAutoDownload(on))
    this.on('change', () => {
      for (const w of windows()) w.webContents.send('updates:change', this.state)
    })
    if (!enabled) return

    autoUpdater.autoDownload = false
    autoUpdater.autoInstallOnAppQuit = true
    autoUpdater.on('checking-for-update', () => this.set({ checking: true, error: null }))
    autoUpdater.on('update-not-available', () => this.checked({ availableVersion: null }))
    autoUpdater.on('update-available', (info) => void this.available(info))
    autoUpdater.on('update-downloaded', (info) => {
      this.set({ staged: true, availableVersion: info.version })
      if (this.userInitiated) void this.install()
      else this.installWhenIdle()
    })
    autoUpdater.on('error', (error) => this.failed(error))

    // A staged update installs on Quit, once the host has actually stopped.
    app.on('before-quit', (event) => {
      if (!this.state.staged || this.prepared) return
      event.preventDefault()
      void this.prepare().then((ready) => ready && app.quit())
    })

    setTimeout(() => this.scheduled(), 30_000)
    setInterval(() => this.scheduled(), 3600_000)
  }

  setAutoCheck(on: boolean) {
    prefs.set('updatesAutoCheck', on)
    this.set({ autoCheck: on })
  }

  setAutoDownload(on: boolean) {
    prefs.set('updatesAutoDownload', on)
    this.set({ autoDownload: on })
  }

  private set(change: Partial<UpdateState>) {
    this.state = { ...this.state, ...change }
    this.emit('change')
  }

  private checked(change: Partial<UpdateState>) {
    const now = Date.now()
    prefs.set('updatesLastCheck', now)
    this.set({ checking: false, lastCheck: now, ...change })
  }

  private failed(error: unknown) {
    this.updateGeneration++
    this.installing = false
    // A preparation still stopping the host restores it after stop completes.
    if (this.prepared) {
      this.prepared = false
      this.host.resumeAfterCancelledUpdate()
    }
    const message = error instanceof Error ? error.message : String(error)
    this.checked({ error: this.userInitiated || this.state.staged ? message : null })
    this.userInitiated = false
  }

  /** Daily, or hourly while a release waits for the iPhone app to pass review. */
  private scheduled() {
    if (!this.state.autoCheck || this.state.checking || this.state.staged) return
    const interval = this.state.waitingForApp ? 3600_000 : 24 * 3600_000
    if (this.state.lastCheck && Date.now() - this.state.lastCheck < interval - 60_000) return
    this.userInitiated = false
    void autoUpdater.checkForUpdates().catch(() => {})
  }

  check() {
    if (!enabled) return
    if (this.state.staged) return void this.install()
    this.userInitiated = true
    this.set({ error: null })
    void autoUpdater.checkForUpdates().catch(() => {})
  }

  /**
   * A release that needs a newer iPhone app than the App Store has (still in review) waits,
   * so paired iPhones aren't asked for an update they can't get yet. Without an answer from
   * the App Store, background checks wait and a check the person started goes ahead.
   */
  private async available(info: UpdateInfo) {
    const required = (info as UpdateInfo & { minApp?: string }).minApp
    let waiting: string | null = null
    if (required) {
      const [hostMinApp, iphones] = await this.localHostFacts()
      this.appStoreVersion = (await appStoreVersion()) ?? this.appStoreVersion
      const storeHas = this.appStoreVersion && parseVersion(this.appStoreVersion) && !isBelow(this.appStoreVersion, required)
      if (isBelow(hostMinApp ?? '0', required) && iphones !== false && !storeHas && !(this.appStoreVersion === null && this.userInitiated)) {
        waiting = required
      }
    }
    this.checked({ availableVersion: info.version, waitingForApp: waiting })
    if (waiting) {
      if (this.userInitiated) this.set({ error: `Codync ${info.version} needs the iPhone app ${waiting}, which isn't in the App Store yet. It installs once that version passes review.` })
      this.userInitiated = false
      return
    }
    if (this.userInitiated || this.state.autoDownload) void autoUpdater.downloadUpdate().catch(() => {})
  }

  /** The local host's `minApp` and whether an iPhone is paired with it. */
  private async localHostFacts(): Promise<[string | null, boolean | null]> {
    try {
      const [hello, devices] = await Promise.all([
        this.host.call<{ minApp?: string }>('hello'),
        this.host.call<{ devices: { platform?: string }[] }>('devices'),
      ])
      return [hello.minApp ?? null, devices.devices.some((d) => d.platform === 'ios')]
    } catch {
      return [null, null]
    }
  }

  /** Waits until the app is in the background, nobody has typed for ten minutes and no bot is working. */
  private installWhenIdle() {
    const timer = setInterval(async () => {
      if (!this.state.staged) return clearInterval(timer)
      if (!this.state.autoDownload || BrowserWindow.getFocusedWindow() || powerMonitor.getSystemIdleTime() < 600) return
      if (!(await this.host.isIdleForUpdate())) return
      clearInterval(timer)
      void this.install()
    }, 30_000)
  }

  private prepare(): Promise<boolean> {
    if (this.prepared) return Promise.resolve(true)
    const generation = this.updateGeneration
    this.preparing ??= (async () => {
      try {
        await this.host.prepareForUpdate()
        if (generation !== this.updateGeneration) {
          this.host.resumeAfterCancelledUpdate()
          return false
        }
        this.prepared = true
        this.set({ error: null })
        return true
      } catch (error) {
        this.set({ error: `Update paused: ${error instanceof Error ? error.message : error}` })
        this.host.resumeAfterCancelledUpdate()
        return false
      } finally {
        this.preparing = null
      }
    })()
    return this.preparing
  }

  private async install() {
    if (this.installing) return
    this.installing = true
    try {
      if (await this.prepare()) autoUpdater.quitAndInstall(true, true)
      else this.installing = false
    } catch (error) {
      this.failed(error)
    }
  }
}

/** The iPhone app's version live in the App Store (iTunes lookup); null when unknown. */
async function appStoreVersion(): Promise<string | null> {
  try {
    const res = await fetch('https://itunes.apple.com/lookup?id=6760984418', { signal: AbortSignal.timeout(10_000), cache: 'no-store' })
    const json = (await res.json()) as { results?: { version?: string }[] }
    return json.results?.[0]?.version ?? null
  } catch {
    return null
  }
}
