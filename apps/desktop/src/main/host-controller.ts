import { execFile, execFileSync } from 'node:child_process'
import { createHash } from 'node:crypto'
import { EventEmitter } from 'node:events'
import { existsSync, promises as fs, readFileSync, realpathSync } from 'node:fs'
import { homedir, platform } from 'node:os'
import { join, resolve } from 'node:path'
import { app } from 'electron'
import type { HostState, LocalHost } from '../shared/ipc'
import { unregisterScreenAgent } from './screen'
import { identity } from './environment'
import { hostEnvironmentError } from '../shared/environment'

/** `CODYNC_PORT` / `CODYNC_HOME` point the app at a dev host started with `codync-host serve`. */
export const devPort = process.env.CODYNC_PORT ? Number(process.env.CODYNC_PORT) : null
export const port = devPort ?? identity.port
export const dataDir = process.env.CODYNC_HOME ?? join(homedir(), identity.dataFolder)
if (identity.environment === 'dev' && (port === 19222 || resolve(dataDir) === resolve(homedir(), '.codync'))) {
  throw new Error('Codync Dev requires its own host port and data directory.')
}
export const baseURL = `http://127.0.0.1:${port}`

interface Health {
  environment?: string
  ok?: boolean
  computerId?: string
  binaryPath?: string
  binaryHash?: string
  busy?: boolean
}

export async function fetchHealth(url = baseURL, timeoutMs = 2000): Promise<Health | null> {
  try {
    const res = await fetch(`${url}/health`, { signal: AbortSignal.timeout(timeoutMs) })
    if (!res.ok) return null
    return (await res.json()) as Health
  } catch {
    return null
  }
}

export function run(bin: string, args: string[]): Promise<{ status: number; output: string }> {
  return new Promise((resolve) => {
    execFile(bin, args, { maxBuffer: 8 * 1024 * 1024, windowsHide: true }, (error, stdout, stderr) => {
      const status = error ? (typeof error.code === 'number' ? error.code : 1) : 0
      resolve({ status, output: `${stdout}${stderr}`.trim() })
    })
  })
}

const isMac = platform() === 'darwin'
const isWindows = platform() === 'win32'
/** macOS and Windows ship the host inside the app; Linux uses the installed one. */
export const bundlesHost = isMac || isWindows

/** The service definition `codync-host install` wrote (`null`: not installed). */
function serviceDefinition(): string | null {
  try {
    if (isWindows) {
      // The per-user Run key that starts the host at sign-in.
      const key = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run'
      return execFileSync('reg', ['query', key, '/v', identity.windowsService], { encoding: 'utf8', windowsHide: true })
    }
    const file = isMac
      ? join(homedir(), `Library/LaunchAgents/${identity.hostLabel}.plist`)
      : join(homedir(), `.config/systemd/user/${identity.systemdUnit}`)
    return readFileSync(file, 'utf8')
  } catch {
    return null
  }
}

const serviceInstalled = () => serviceDefinition() !== null

/**
 * Manages the local codync-host (binary, background service, health) the way the native
 * apps did: bundled next to the app, else a Homebrew / cargo install.
 */
export class HostController extends EventEmitter {
  state: HostState = { kind: 'starting' }
  local: LocalHost | null = null
  private loop: NodeJS.Timeout | null = null
  private generation = 0
  private installing = false
  private replacedStale = false
  private expectedHash: string | null = null
  private failures = 0
  private preparingForUpdate = false

  get binary(): string | null {
    const candidates = [
      app.isPackaged && isMac ? join(process.resourcesPath, 'codync-host') : null,
      app.isPackaged && isWindows ? join(process.resourcesPath, 'codync-host.exe') : null,
      process.env.CODYNC_HOST_BIN ?? null,
      ...(identity.environment === 'dev' ? [
        join(__dirname, '../../build/native/codync-host'),
        join(__dirname, '../../../../host/target/debug/codync-host'),
        join(homedir(), '.local/bin/codync-dev-host'),
      ] : [
        join(homedir(), '.local/bin/codync-host'),
        '/opt/homebrew/bin/codync-host',
        '/usr/local/bin/codync-host',
        '/usr/bin/codync-host',
        join(homedir(), '.cargo/bin/codync-host'),
      ]),
    ]
    return candidates.find((c): c is string => !!c && existsSync(c)) ?? null
  }

  get logPath() {
    return join(dataDir, 'host.log')
  }

  private setState(state: HostState) {
    this.state = state
    this.emit('change')
  }

  private setLocal(local: LocalHost | null) {
    this.local = local
    this.emit('change')
  }

  start() {
    void this.refresh()
  }

  private uninstalled() {
    return prefs.get('hostUninstalled') === true
  }

  async refresh() {
    if (this.preparingForUpdate) return
    if (!this.binary && devPort === null) return this.setState({ kind: 'missingBinary' })
    if (devPort === null && prefs.get('hostRestartAfterAppUpdate') === true && serviceInstalled()) return this.install()
    if (devPort === null && !serviceInstalled()) {
      // The app is the way to run the host: set it up right away, unless the user took it out.
      if (this.uninstalled()) return this.setState({ kind: 'notInstalled' })
      return this.install()
    }
    // A service left by another copy of the app (moved, deleted or replaced) can't start this
    // app's host; the health check only catches that once something answers.
    if (devPort === null && this.binary && !serviceRuns(this.binary) && !this.uninstalled()) return this.install()
    this.connect()
  }

  private async verifyBinary(bin: string) {
    const result = await run(bin, ['compat'])
    if (result.status !== 0) throw new Error(result.output || 'Cannot verify the host environment.')
    const info = JSON.parse(result.output) as { environment?: string }
    const mismatch = hostEnvironmentError(identity.environment, info.environment)
    if (mismatch) throw new Error(mismatch)
  }

  /** A service operation must never invoke the other environment's binary. */
  async install() {
    const bin = this.binary
    if (this.preparingForUpdate || this.installing || !bin || devPort !== null) return
    this.installing = true
    this.stopLoop()
    prefs.set('hostUninstalled', false)
    this.setState({ kind: 'starting' })
    try {
      await this.verifyBinary(bin)
      const result = await run(bin, ['install', '--port', String(port)])
      if (result.status !== 0) {
        this.setState({ kind: 'failed', message: result.output || 'Install failed' })
      } else {
        await sleep(1000)
        this.connect()
      }
    } catch (error) {
      this.setState({ kind: 'failed', message: error instanceof Error ? error.message : String(error) })
    } finally {
      this.installing = false
    }
  }

  restart() {
    if (devPort !== null) {
      this.setState({ kind: 'failed', message: 'Restart the manually started development host in its terminal.' })
      return
    }
    this.replacedStale = false
    void this.install()
  }

  /** One loopback API call from the main process; throws when the host isn't running. */
  async call<T>(method: string, params: object = {}, timeoutMs = 5000): Promise<T> {
    const local = this.local
    if (!local) throw new Error('The host isn’t running')
    const res = await fetch(`${local.baseURL}/api/${method}`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${local.token}`, 'Content-Type': 'application/json' },
      body: JSON.stringify(params),
      signal: AbortSignal.timeout(timeoutMs),
    })
    if (!res.ok) throw new Error(`${method}: HTTP ${res.status}`)
    return (await res.json()) as T
  }

  async uninstall(): Promise<boolean> {
    const bin = this.binary
    if (!bin || devPort !== null) return false
    try {
      await this.verifyBinary(bin)
      this.stopLoop()
      const result = await run(bin, ['uninstall'])
      if (result.status !== 0) throw new Error(result.output || 'Uninstall failed')
      prefs.set('hostUninstalled', true)
      this.setLocal(null)
      this.setState({ kind: 'notInstalled' })
      return true
    } catch (error) {
      this.setState({ kind: 'failed', message: error instanceof Error ? error.message : String(error) })
      return false
    }
  }

  /**
   * Stops the service before the app replaces itself; `resumeAfterCancelledUpdate` undoes it.
   * macOS and Windows: there the host runs from inside the app (and Windows can't replace a
   * running program). Linux uses the installed host, which an app update leaves alone.
   */
  async prepareForUpdate() {
    if (devPort !== null || !bundlesHost) return
    if (this.installing) throw new Error('The host is being installed. Try again when it finishes.')
    this.preparingForUpdate = true
    if (serviceInstalled()) prefs.set('hostRestartAfterAppUpdate', true)
    this.stopLoop()
    unregisterScreenAgent()
    const bin = this.binary
    if (!bin) throw new Error('The bundled host is missing.')
    await this.verifyBinary(bin)
    const result = await run(bin, ['stop'])
    if (result.status !== 0) throw new Error(result.output || 'The old host could not be stopped.')
  }

  resumeAfterCancelledUpdate() {
    if (!this.preparingForUpdate) return
    this.preparingForUpdate = false
    if (serviceInstalled()) void this.install()
    else void this.refresh()
  }

  async isIdleForUpdate(): Promise<boolean> {
    if (this.installing || this.preparingForUpdate || devPort !== null) return false
    if (!serviceInstalled()) return true
    const health = await fetchHealth()
    return health?.busy === false
  }

  private stopLoop() {
    this.generation++
    if (this.loop) clearTimeout(this.loop)
    this.loop = null
  }

  /** Watches the host's health; the renderer's store handles the event stream itself. */
  private connect() {
    if (this.preparingForUpdate) return
    this.stopLoop()
    const generation = this.generation
    this.failures = 0
    const tick = async () => {
      if (generation !== this.generation) return
      if (await this.attachLocal()) {
        this.failures = 0
        if (generation !== this.generation) return
        this.setState({ kind: 'running' })
        if (devPort === null && prefs.get('hostRestartAfterAppUpdate') === true) prefs.set('hostRestartAfterAppUpdate', false)
      } else {
        this.failures++
        if (this.failures > 5) this.setState({ kind: 'failed', message: "The host isn't responding. See the log for details." })
        else if (this.state.kind !== 'running') this.setState({ kind: 'starting' })
      }
      if (generation === this.generation) this.loop = setTimeout(tick, 5000)
    }
    void (async () => {
      const bin = this.binary
      if (bin && devPort === null) this.expectedHash = await hashFile(bin)
      void tick()
    })()
  }

  /** Attaches the local host once it answers, and again when its token or identity changed. */
  private async attachLocal(): Promise<boolean> {
    const health = await fetchHealth()
    if (!health?.computerId) return false
    const mismatch = hostEnvironmentError(identity.environment, health.environment)
    if (mismatch) {
      this.setState({ kind: 'failed', message: mismatch })
      return false
    }
    const token = await readToken()
    if (!token) return false
    if (devPort === null) {
      // A rebuild can change the host without changing the release version.
      const bin = this.binary
      if (!this.expectedHash || !bin) return false
      const matches = health.binaryHash === this.expectedHash && health.binaryPath === resolved(bin)
      if (!matches) {
        if (!this.replacedStale) {
          this.replacedStale = true
          void this.install()
        }
        return false
      }
    }
    if (this.local?.computerId === health.computerId && this.local.token === token) return true
    this.setLocal({ computerId: health.computerId, baseURL, token })
    return true
  }
}

async function readToken(): Promise<string | null> {
  try {
    return (await fs.readFile(join(dataDir, 'token'), 'utf8')).trim() || null
  } catch {
    return null
  }
}

async function hashFile(path: string): Promise<string | null> {
  try {
    return createHash('sha256').update(await fs.readFile(path)).digest('hex')
  } catch {
    return null
  }
}

function resolved(path: string) {
  try {
    return realpathSync(path)
  } catch {
    return path
  }
}

/** Whether the installed service starts this binary. */
function serviceRuns(bin: string) {
  const service = serviceDefinition()
  if (service === null) return false
  // Windows' service starts the launcher next to the host (codync-hostw.exe).
  const runs = (path: string) => service.includes(isWindows ? path.replace(/codync-host\.exe$/, identity.supervisor) : path)
  return runs(bin) || runs(resolved(bin))
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

/** Main-process preferences (a small JSON file in the app's data folder). */
export const prefs = (() => {
  const file = () => join(app.getPath('userData'), 'host-prefs.json')
  let cache: Record<string, unknown> | null = null
  const load = () => {
    if (cache) return cache
    try {
      cache = JSON.parse(readFileSync(file(), 'utf8')) as Record<string, unknown>
    } catch {
      cache = {}
    }
    return cache
  }
  return {
    get: (key: string) => load()[key],
    set: (key: string, value: unknown) => {
      const data = load()
      data[key] = value
      void fs.mkdir(app.getPath('userData'), { recursive: true }).then(() => fs.writeFile(file(), JSON.stringify(data)))
    },
  }
})()
