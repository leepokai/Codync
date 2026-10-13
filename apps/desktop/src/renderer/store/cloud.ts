import type { CloudStatus, ClaimSignature } from '@shared/models'
import { Observable } from '../lib/observable'
import { pref } from '../lib/prefs'
import { account } from './account'
import type { AppModel } from './app-model'
import type { BotStore } from './bot-store'

// Wire types of the Codync cloud (`/v1`, cloud/src/api.ts); times are epoch milliseconds.

/** A computer in the account (`GET /v1/computers`). */
export interface CloudComputer {
  computerId: string
  name: string
  platform?: string | null
  device?: string | null
  signKey: string
  boxKey?: string | null
  version?: string | null
  online?: boolean | null
  lastSeenAt?: number | null
  claimedAt?: number | null
  /** `none` · `pending` · `granted`. */
  access?: string | null
}

export interface CloudDevice {
  deviceId: string
  deviceKey: string
  name: string
  platform?: string | null
  createdAt?: number | null
  lastUsedAt?: number | null
  revoked?: boolean | null
}

export interface CloudGrant {
  grantId: string
  deviceId: string
  deviceName?: string | null
  platform?: string | null
  scopes?: string[] | null
  createdAt?: number | null
}

export class CloudError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string | null,
  ) {
    super(
      code === 'requestExpired' ? 'The request expired. Ask again.'
      : code === 'accountDeleted' ? 'This account was deleted.'
      : code === 'rateLimited' ? 'Too many requests. Try again in a little while.'
      : (message ?? `Codync cloud error (${code})`),
    )
  }

  /** A busy or failing cloud, or a session token that wasn't fresh yet: trying again later settles it. */
  get isTransient() {
    return this.status === 0 || this.status >= 500 || this.status === 429 || this.status === 401
  }
}

async function request<T>(method: string, path: string, body?: unknown, signed = false): Promise<T> {
  const res = await window.codync.cloud.request(method, path, body, signed)
  if (!res.ok) throw new CloudError(res.error.status, res.error.code, res.error.message)
  return res.value as T
}

/** Host identities whose cloud default has been applied (or found already on). */
const cloudDefaultApplied = pref<string[]>('cloudDefaultApplied', [])
/** The identities this computer claimed: a later key reset can clean them up. */
const claimedComputerIds = pref<string[]>('claimedComputerIds', [])
const keptOut = (userId: string) => pref<string[]>(`keptOutOfAccount.${userId}`, [])

/**
 * The signed-in account's side of this computer (mirrors kit's `CloudClient`): registering this device, the account's computers, claiming this
 * computer's host, and turning its cloud on by default.
 */
export class CloudModel extends Observable {
  computers: CloudComputer[] = []
  private registeredFor: string | null = null
  private enablingCloud = false
  private autoClaiming = false

  constructor(private app: AppModel) {
    super()
    account.subscribe(() => this.tick())
    app.subscribe(() => this.tick())
  }

  private fail(error: unknown, quiet = false) {
    if (quiet || (error instanceof CloudError && error.isTransient)) return
    this.app.setError(error instanceof Error ? error.message : String(error))
  }

  private get userId() {
    return account.user?.userId ?? null
  }

  /** Re-evaluated on every account or store change. */
  private tick() {
    const user = this.userId
    if (!user) {
      if (this.computers.length) {
        this.computers = []
        this.changed()
      }
      this.registeredFor = null
      return
    }
    if (this.registeredFor !== user) {
      this.registeredFor = user
      // Signed in: make this computer known to the account, then list its computers.
      void (async () => {
        try {
          await request('POST', 'v1/devices', { name: await deviceName(), platform: { darwin: 'macos', linux: 'linux', win32: 'windows' }[window.codync.platform] }, true)
        } catch (error) {
          this.fail(error)
        }
        await this.refresh()
      })()
    }
    this.applyCloudDefault()
    this.autoClaim()
  }

  async refresh() {
    if (!this.userId) return
    try {
      const res = await request<{ computers: CloudComputer[] }>('GET', 'v1/computers', undefined, true)
      this.computers = res.computers
      this.changed()
    } catch (error) {
      this.fail(error)
    }
  }

  /**
   * The cloud is on by default, so a computer without Tailscale is reachable away from home. It's
   * turned on once per host identity, the first time it's seen off; after that off stays off.
   */
  private applyCloudDefault() {
    const store = this.app.local
    if (!account.cloudURL || !store || store.connection.kind !== 'online' || !store.cloud) return
    const id = store.computer.id
    if (cloudDefaultApplied.get().includes(id)) return
    cloudDefaultApplied.set([...cloudDefaultApplied.get(), id])
    if (store.cloud.enabled || this.enablingCloud) return
    this.enablingCloud = true
    void this.enableCloud(store)
      .catch(() => {})
      .finally(() => (this.enablingCloud = false))
  }

  /** Signed in, this computer joins the account on its own, unless the user took it out of it. */
  private autoClaim() {
    const user = this.userId
    const store = this.app.local
    if (user && store && store.cloud?.owner?.userId === user) claimedComputerIds.set([store.computer.id])
    const target = this.autoClaimTarget
    if (!target || this.autoClaiming) return
    this.autoClaiming = true
    void (async () => {
      // A failed join (network hiccup) is retried with backoff for as long as it's still wanted.
      let delay = 30_000
      while (!(await this.claim(target, true))) {
        await new Promise((r) => setTimeout(r, delay))
        delay = Math.min(delay * 2, 600_000)
        if (this.autoClaimTarget?.computer.id !== target.computer.id) break
      }
      this.autoClaiming = false
    })()
  }

  private get autoClaimTarget(): BotStore | null {
    // Only with the cloud on: joining needs it, and turning it back on is never done silently.
    const user = this.userId
    const store = this.app.local
    if (!user || !store || store.connection.kind !== 'online') return null
    const status = store.cloud
    if (!status?.enabled || status.owner || keptOut(user).get().includes(store.computer.id)) return null
    return store
  }

  /** §4.2 A: makes a computer this app manages part of the account; its cloud is turned on first. */
  async claim(store: BotStore, quiet = false): Promise<boolean> {
    const user = this.userId
    if (!user || !store.client) {
      if (!quiet) this.app.setError('Sign in first.')
      return false
    }
    const out = keptOut(user)
    out.set(out.get().filter((x) => x !== store.computer.id))
    try {
      const client = await store.ready()
      if (!store.cloud?.enabled) await this.enableCloud(store)
      const challenge = await request<{ claimId: string; nonce: string }>('POST', 'v1/claims', {})
      const signed = await client.call<ClaimSignature>('claimSign', { claimId: challenge.claimId, nonce: challenge.nonce, userId: user })
      await request('POST', `v1/claims/${challenge.claimId}/complete`, signed)
      await this.refresh()
      if (store === this.app.local) await this.forgetEarlierIdentities(store.computer.id)
      return true
    } catch (error) {
      this.fail(error, quiet)
      return false
    }
  }

  /**
   * This computer's host got new keys (a reset data folder, a reinstall): the identities claimed
   * before are dead copies of it, so they leave the account instead of showing up twice.
   */
  private async forgetEarlierIdentities(current: string) {
    const earlier = new Set(claimedComputerIds.get().filter((id) => id !== current))
    for (const c of this.computers) if (earlier.has(c.computerId) && !c.online) await this.removeFromAccount(c.computerId)
    claimedComputerIds.set([current])
  }

  async unclaim(store: BotStore) {
    const user = this.userId
    if (user) {
      const out = keptOut(user)
      if (!out.get().includes(store.computer.id)) out.set([...out.get(), store.computer.id])
    }
    try {
      await (await store.ready()).call('unclaim', {}, 30_000)
      await this.refresh()
    } catch (error) {
      this.fail(error)
    }
  }

  async setCloud(store: BotStore, enabled: boolean) {
    if (!cloudDefaultApplied.get().includes(store.computer.id)) cloudDefaultApplied.set([...cloudDefaultApplied.get(), store.computer.id])
    try {
      if (enabled) await this.enableCloud(store)
      else await (await store.ready()).call<CloudStatus>('setCloud', { enabled: false }, 30_000)
    } catch (error) {
      this.fail(error)
    }
  }

  /** The host's cloud comes with its build; this only turns it on. */
  private async enableCloud(store: BotStore) {
    return (await store.ready()).call<CloudStatus>('setCloud', { enabled: true }, 30_000)
  }

  async removeFromAccount(computerId: string) {
    try {
      await request('DELETE', `v1/computers/${computerId}`)
      await this.refresh()
    } catch (error) {
      this.fail(error)
    }
  }

  /** An offline account computer named like one this app uses or sees online: an earlier identity of it. */
  isOlderCopy(c: CloudComputer) {
    if (c.online) return false
    return this.app.computers.some((x) => x.id !== c.computerId && x.name === c.name) ||
      this.computers.some((o) => o.computerId !== c.computerId && o.name === c.name && (this.app.store(o.computerId) !== null || !!o.online))
  }

  devices() {
    return request<{ devices: CloudDevice[] }>('GET', 'v1/devices').then((r) => r.devices)
  }

  revokeDevice(deviceId: string) {
    return request('DELETE', `v1/devices/${deviceId}`)
  }

  grants(computerId: string) {
    return request<{ grants: CloudGrant[] }>('GET', `v1/computers/${computerId}/grants`).then((r) => r.grants)
  }

  revokeGrant(computerId: string, grantId: string) {
    return request('DELETE', `v1/computers/${computerId}/grants/${grantId}`)
  }

  async renameComputer(computerId: string, name: string) {
    await request('PATCH', `v1/computers/${computerId}`, { name })
    await this.refresh()
  }
}

/** This computer's name for the account's device list. */
async function deviceName() {
  return window.codync.computerName || { darwin: 'Mac', linux: 'Linux', win32: 'Windows' }[window.codync.platform]
}
