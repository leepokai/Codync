import { statSync } from 'node:fs'
import { join } from 'node:path'

/** A host's cloud state, as `cloudStatus` reports it (`tokenSet` is absent while the cloud is off). */
export type CloudStatus = {
  enabled: boolean
  url?: string | null
  tokenSet?: boolean
  owner?: unknown
}

export type CloudCall = (method: string, params: Record<string, unknown>) => Promise<unknown>

/** The sandbox to use: its data folder and the port its host answers on. */
export type Sandbox = { home: string; port: number; liveHostId?: () => Promise<string> }

/** The keys `cloud_url.rs` saves while the cloud is configured; `cloud_enabled` records the switch. */
export const CLOUD_KEYS = ['cloud_url', 'cloud_relay_token'] as const
/** Variables that would point a child host at another cloud: presence alone is refused, values are never read. */
export const CLOUD_ENV = ['CODYNC_CLOUD_URL', 'CODYNC_CLOUD_TOKEN'] as const

type StoredCloud = { hostId: string; cloudKeys: string[] }

type Deps = {
  stat: (path: string) => { isDirectory(): boolean }
  hostKeys: (dbPath: string) => StoredCloud
  health: (port: number) => Promise<{ hostId?: string }>
  env: () => string[]
}

/** The host keeps its id and any saved cloud URL/token in `kv`; read key names, never their values. */
function hostKeys(dbPath: string): StoredCloud {
  // Loaded lazily: node:sqlite is new and only these tests need it.
  const { DatabaseSync } = process.getBuiltinModule('node:sqlite') as typeof import('node:sqlite')
  const db = new DatabaseSync(dbPath, { readOnly: true })
  try {
    const id = db.prepare("SELECT v FROM kv WHERE k = 'host_id'").get() as { v?: string } | undefined
    const placeholders = CLOUD_KEYS.map(() => '?').join(', ')
    const rows = db.prepare(`SELECT k FROM kv WHERE k IN (${placeholders})`).all(...CLOUD_KEYS) as { k: string }[]
    return { hostId: id?.v ?? '', cloudKeys: rows.map((row) => row.k) }
  } finally {
    db.close()
  }
}

async function health(port: number): Promise<{ hostId?: string }> {
  const response = await fetch(`http://127.0.0.1:${port}/health`, { signal: AbortSignal.timeout(5_000) })
  if (!response.ok) throw new Error(`health -> ${response.status}`)
  return await response.json() as { hostId?: string }
}

/**
 * Proves the sandbox is the one this test may change: no saved cloud URL or relay token in its `kv`
 * (key names only, never values), no `CODYNC_CLOUD_URL`/`CODYNC_CLOUD_TOKEN` override (presence only),
 * and the stored host id equals the one `/health` reports on that folder's port. The switch is left
 * alone: a `--cloud` sandbox starts with the development cloud on, and a run puts it back the way it
 * found it. Returns the id so the caller can check the host the app actually talks to as well.
 */
export async function verifySandbox(sandbox: Sandbox, deps: Partial<Deps> = {}): Promise<{ hostId: string }> {
  const stat = deps.stat ?? statSync
  const readKeys = deps.hostKeys ?? hostKeys
  const readHealth = deps.health ?? health
  const readEnv = deps.env ?? (() => CLOUD_ENV.filter((key) => key in process.env))
  if (!stat(sandbox.home).isDirectory()) throw new Error(`the sandbox data folder ${sandbox.home} is not a directory`)
  const overrides = readEnv()
  if (overrides.length > 0) throw new Error(`the sandbox environment sets ${overrides.join(', ')}; use a clean environment`)
  let stored
  try {
    stored = readKeys(join(sandbox.home, 'codync.db'))
  } catch (error) {
    throw new Error(`cannot read ${join(sandbox.home, 'codync.db')}: ${String(error).slice(0, 120)}`)
  }
  if (stored.cloudKeys.length > 0) {
    throw new Error(`the sandbox already holds ${stored.cloudKeys.join(', ')}; use a fresh data folder`)
  }
  if (!stored.hostId) throw new Error('the sandbox database has no host id')
  const live = await readHealth(sandbox.port)
  if (live.hostId !== stored.hostId) {
    throw new Error(`the sandbox folder belongs to host ${stored.hostId}, but port ${sandbox.port} answers as ${live.hostId ?? 'nothing'}`)
  }
  return { hostId: stored.hostId }
}

/** What a dedicated sandbox looks like when we may use its cloud: no account, no saved credential. */
function usable(status: CloudStatus) {
  return !status.owner && !status.tokenSet
}

/** A `--cloud` sandbox starts with the development cloud on; a run puts the switch back as it found it. */
function sameSwitch(before: CloudStatus, after: CloudStatus) {
  return after.enabled === before.enabled
}

/**
 * Points the sandbox's cloud at `url` for the run and puts it back afterwards. The sandbox is verified
 * first (see `verifySandbox`); `liveHostId` names the host the app is actually connected to, which must
 * be the same one, both before and again just before the cloud is changed. Cleanup always runs, and a
 * cleanup that fails is reported even when the run itself failed.
 */
export async function withLoopbackCloud<T>(
  call: CloudCall,
  sandbox: Sandbox,
  url: string,
  run: () => Promise<T>,
  verify: (sandbox: Sandbox) => Promise<{ hostId: string }> = verifySandbox,
): Promise<T> {
  const stored = await verify(sandbox)
  const sameHost = async () => {
    if (!sandbox.liveHostId) return
    const connected = await sandbox.liveHostId()
    if (connected !== stored.hostId) {
      throw new Error(`the app is connected to host ${connected}, not the sandbox's ${stored.hostId}`)
    }
  }
  await sameHost()
  // Every write, including cleanup, re-checks the host right before it goes out: a connection that
  // moved between the check and the call is refused instead of written to another host.
  const guarded = async (method: string, params: Record<string, unknown>) => {
    await sameHost()
    return call(method, params)
  }
  const before = await call('cloudStatus', {}) as CloudStatus
  if (!usable(before)) {
    throw new Error(
      `use a dedicated sandbox: the cloud must have no account and no saved token (enabled=${before.enabled}, owner=${Boolean(before.owner)}, tokenSet=${Boolean(before.tokenSet)})`,
    )
  }
  let primary: unknown
  try {
    await sameHost()
    await guarded('setCloudUrl', { url })
    await guarded('setCloud', { enabled: true })
    return await run()
  } catch (error) {
    primary = error
    throw error
  } finally {
    const problems: string[] = []
    const restore: [string, Record<string, unknown>][] = [['setCloud', { enabled: false }], ['setCloudUrl', { url: '' }]]
    // A `--cloud` sandbox had the development cloud on: leave it that way, or the next run sees a sandbox
    // this one changed.
    if (before.enabled) restore.push(['setCloud', { enabled: true }])
    for (const [method, params] of restore) {
      try {
        await guarded(method, params)
      } catch (error) {
        problems.push(`${method}: ${String(error).slice(0, 120)}`)
      }
    }
    try {
      const after = await call('cloudStatus', {}) as CloudStatus
      if (!usable(after)) {
        problems.push(`still on: enabled=${after.enabled} owner=${Boolean(after.owner)} tokenSet=${Boolean(after.tokenSet)}`)
      }
      if (!sameSwitch(before, after)) {
        problems.push(`the switch is ${after.enabled ? 'on' : 'off'}, not ${before.enabled ? 'on' : 'off'}`)
      }
    } catch (error) {
      problems.push(`cloudStatus: ${String(error).slice(0, 120)}`)
    }
    if (problems.length > 0) {
      const cleanup = new Error(`cloud cleanup failed: ${problems.join('; ')}`)
      if (primary) throw new AggregateError([primary, cleanup], cleanup.message)
      throw cleanup
    }
  }
}

export type BridgeCall = {
  call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown>
}

export type PinnedHost = { baseURL: string; token: string }

/**
 * Calls the host captured once, at its own base URL, never a host picked again at call time: a
 * reconnection elsewhere cannot move a write or a cleanup to another computer.
 */
export async function pinnedHostCall<T = unknown>(
  bridge: BridgeCall,
  pin: PinnedHost,
  method: string,
  params: Record<string, unknown>,
): Promise<T> {
  if (!pin.baseURL || !pin.token) throw new Error('no pinned host session')
  return (await bridge.call(pin.baseURL, pin.token, method, params, 30_000)) as T
}
