import { execFile, execFileSync, spawn } from 'node:child_process'
import { EventEmitter } from 'node:events'
import { existsSync } from 'node:fs'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { app, BrowserWindow, ipcMain, powerMonitor, shell } from 'electron'
import electronUpdater, { type UpdateInfo } from 'electron-updater'
import type { UpdateState } from '../shared/ipc'
import { isMainEnvironment } from './account'
import { identity } from './environment'
import { prefs, type HostController } from './host-controller'

const { autoUpdater } = electronUpdater

/**
 * The app's own updates (electron-updater, GitHub releases; packaged `main` builds only, so a
 * local `dev` build never replaces itself with a release). Codync
 * coordinates its background services before the installer replaces the app: the host is
 * stopped first, and a restart marker makes the next launch reinstall it from the new bundle.
 */
const enabled = app.isPackaged && isMainEnvironment()
const shipIt = `${identity.appId}.ShipIt`

const bundlePath = () => join(app.getPath('exe'), '../../..')

/** How long another device's update request lasts: as long as that device shows it updating. */
const REQUEST_WAIT = 10 * 60_000

/**
 * macOS's installer (ShipIt) cancels when the old app runs again while it swaps the bundle
 * ("App Still Running"), and someone who sees no window for those seconds opens it. An old copy
 * launched during an install quits at once and opens the app again once the installer stops:
 * the new release after an install, this one after a failed attempt.
 */
export function yieldsToInstall(): boolean {
  const pending = prefs.get('updateInstalling') as { version: string; at: number } | undefined
  if (!pending) return false
  const installing = pending.version !== app.getVersion() && Date.now() - pending.at < 120_000 && shipItRunning()
  if (installing) reopenAfterInstall()
  else prefs.set('updateInstalling', undefined)
  return installing
}

/** A detached wait (two minutes at most) for the installer job to stop, then `open`. */
function reopenAfterInstall() {
  const script = 'for _ in $(seq 120); do /bin/launchctl list "$0" 2>/dev/null | grep -q \'"PID" =\' || break; sleep 1; done; exec /usr/bin/open "$1"'
  spawn('/bin/sh', ['-c', script, shipIt, bundlePath()], { detached: true, stdio: 'ignore' }).unref()
}

export class Updates extends EventEmitter<{ change: []; upToDate: []; setAutoCheck: [on: boolean]; setAutoDownload: [on: boolean] }> {
  state: UpdateState = {
    supported: enabled,
    checking: false,
    availableVersion: null,
    downloadPercent: null,
    staged: false,
    installing: false,
    autoCheck: prefs.get('updatesAutoCheck') !== false,
    // On unless turned off: installs still wait until Codync is idle.
    autoDownload: prefs.get('updatesAutoDownload') !== false,
    lastCheck: (prefs.get('updatesLastCheck') as number | undefined) ?? null,
    error: null,
  }
  private userInitiated = false
  /** The person chose Update: restart into the release as soon as it's downloaded. */
  private installWhenDownloaded = false
  /**
   * Until when another device's request to update this computer (`installHostUpdate`) lasts: the
   * release downloads, and installs once no bot is working without waiting for the person.
   */
  private requestedUntil = 0
  private preparing: Promise<boolean> | null = null
  private prepared = false
  private updateGeneration = 0

  constructor(private host: HostController) {
    super()
  }

  register(windows: () => BrowserWindow[]) {
    ipcMain.handle('updates:state', () => this.state)
    ipcMain.on('updates:check', () => this.check())
    ipcMain.on('updates:install', () => this.installNow())
    ipcMain.on('updates:autoCheck', (_e, on: boolean) => this.setAutoCheck(on))
    ipcMain.on('updates:autoDownload', (_e, on: boolean) => this.setAutoDownload(on))
    this.on('setAutoCheck', (on: boolean) => this.setAutoCheck(on))
    this.on('setAutoDownload', (on: boolean) => this.setAutoDownload(on))
    this.on('change', () => {
      for (const w of windows()) w.webContents.send('updates:change', this.state)
    })
    removeStuckShipIt()
    if (!enabled) return
    // macOS relaunches the updated app from the background, behind the app in front, and ignores
    // `app.focus` there (cooperative activation); opening its own bundle brings it forward.
    if (prefs.get('updateRelaunchInFront') === true) {
      prefs.set('updateRelaunchInFront', false)
      if (process.platform === 'darwin') void shell.openPath(bundlePath())
    }

    this.host.on('updateRequested', () => this.installRequested())
    autoUpdater.autoDownload = false
    autoUpdater.autoInstallOnAppQuit = true
    autoUpdater.on('checking-for-update', () => this.set({ checking: true, error: null }))
    autoUpdater.on('update-not-available', () => this.upToDate())
    autoUpdater.on('update-available', (info) => this.available(info))
    autoUpdater.on('download-progress', ({ percent }) => {
      const rounded = Math.floor(percent)
      if (this.state.downloadPercent !== null && rounded !== this.state.downloadPercent) this.set({ downloadPercent: rounded })
    })
    // Ready to install: the person restarts from the Updates page, or it installs once idle.
    autoUpdater.on('update-downloaded', (info) => {
      this.set({ staged: true, downloadPercent: null, availableVersion: info.version })
      if (!this.installWhenDownloaded) return this.installWhenIdle()
      this.installWhenDownloaded = false
      void this.install(true)
    })
    // The latest release has no update feed for this platform until its build finishes
    // uploading: nothing to offer yet.
    autoUpdater.on('error', (error: Error & { code?: string }) =>
      error.code === 'ERR_UPDATER_CHANNEL_FILE_NOT_FOUND' ? this.upToDate() : this.failed(error))

    // A staged update installs on Quit, once the host has actually stopped.
    app.on('before-quit', (event) => {
      if (!this.state.staged || this.prepared) return
      event.preventDefault()
      void this.prepare().then((ready) => ready && app.quit())
    })

    setTimeout(() => this.scheduled(), 30_000)
    setInterval(() => {
      removeStuckShipIt()
      this.scheduled()
    }, 3600_000)
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

  /** A check the person asked for answers even when there's nothing new (`upToDate`). */
  private upToDate() {
    this.installWhenDownloaded = false
    this.requestedUntil = 0
    const asked = this.userInitiated
    this.userInitiated = false
    this.checked({ availableVersion: null })
    if (asked) this.emit('upToDate')
  }

  private failed(error: unknown) {
    this.updateGeneration++
    this.installWhenDownloaded = false
    this.requestedUntil = 0
    prefs.set('updateRelaunchInFront', false)
    prefs.set('updateInstalling', undefined)
    // A preparation still stopping the host restores it after stop completes.
    if (this.prepared) {
      this.prepared = false
      this.host.resumeAfterCancelledUpdate()
    }
    const message = error instanceof Error ? error.message : String(error)
    this.checked({ downloadPercent: null, installing: false, error: this.userInitiated || this.state.staged ? message : null })
    this.userInitiated = false
  }

  /** Checks daily; the minute of slack keeps the hourly timer from skipping a day. */
  private scheduled() {
    if (!this.state.autoCheck || this.state.checking || this.state.staged) return
    if (this.state.lastCheck && Date.now() - this.state.lastCheck < 24 * 3600_000 - 60_000) return
    this.userInitiated = false
    void autoUpdater.checkForUpdates().catch(() => {})
  }

  /** Checks, downloads, or once a release is staged, restarts into it. */
  check() {
    if (!enabled) return
    if (this.state.staged) return void this.install(true)
    if (this.state.downloadPercent !== null) return
    this.userInitiated = true
    this.set({ error: null })
    void autoUpdater.checkForUpdates().catch(() => {})
  }

  /** The sidebar's Update badge: restarts into the available release, downloading it first. */
  installNow() {
    if (!enabled || !this.state.availableVersion) return
    if (this.state.staged) return void this.install(true)
    this.installWhenDownloaded = true
    this.check()
  }

  /** Another device asked this computer to update: download the release and restart into it in the background. */
  private installRequested() {
    this.requestedUntil = Date.now() + REQUEST_WAIT
    if (this.state.staged) return this.installWhenIdle()
    if (this.state.checking || this.state.downloadPercent !== null) return
    void autoUpdater.checkForUpdates().catch(() => {})
  }

  private get requested() {
    return Date.now() < this.requestedUntil
  }

  /**
   * The release GitHub marks latest, which the release gate gives only to one the App Store's
   * iPhone app works with (docs/reference/compatibility.md).
   */
  private available(info: UpdateInfo) {
    this.checked({ availableVersion: info.version })
    if (this.userInitiated || this.state.autoDownload || this.requested) {
      this.set({ downloadPercent: 0 })
      void autoUpdater.downloadUpdate().catch(() => {})
    }
  }

  /**
   * Waits until no bot is working and, unless another device asked for the update, the app is in
   * the background and nobody has typed for ten minutes.
   */
  private installWhenIdle() {
    const timer = setInterval(async () => {
      if (!this.state.staged) return clearInterval(timer)
      if (!this.requested && (!this.state.autoDownload || BrowserWindow.getFocusedWindow() || powerMonitor.getSystemIdleTime() < 600)) return
      if (!(await this.host.isIdleForUpdate())) return
      clearInterval(timer)
      void this.install(false)
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
        // Both installer paths (Restart, and Quit with a staged update) pass here right before quitting.
        prefs.set('updateInstalling', { version: this.state.availableVersion, at: Date.now() })
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

  /** `inFront`: the person chose to restart, so the relaunched app comes forward. */
  private async install(inFront: boolean) {
    if (this.state.installing) return
    this.set({ installing: true })
    try {
      if (!(await this.prepare())) return this.set({ installing: false })
      if (inFront) prefs.set('updateRelaunchInFront', true)
      autoUpdater.quitAndInstall(true, true)
    } catch (error) {
      this.failed(error)
    }
  }
}

/**
 * macOS's installer job (Squirrel's ShipIt) restarts every 2 s until it exits cleanly, so once its
 * update request is gone (a cleaned Caches folder) it fails forever and its log grows without
 * bound. An idle job without a request can't install anything: remove it.
 */
function removeStuckShipIt() {
  if (process.platform !== 'darwin') return
  if (existsSync(join(homedir(), 'Library/Caches', shipIt, 'ShipItState.plist'))) return
  execFile('/bin/launchctl', ['list', shipIt], (error, stdout) => {
    // Not submitted, or running right now (the next check finds it idle).
    if (error || stdout.includes('"PID" =')) return
    execFile('/bin/launchctl', ['remove', shipIt], () => {})
  })
}

function shipItRunning() {
  if (process.platform !== 'darwin') return false
  try {
    return execFileSync('/bin/launchctl', ['list', shipIt], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).includes('"PID" =')
  } catch {
    // Not submitted.
    return false
  }
}
