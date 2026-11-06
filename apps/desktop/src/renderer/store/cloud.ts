import type { CloudStatus, ClaimSignature } from '@shared/models'
import { mayAutoClaim } from '../lib/auto-claim'
import { Observable } from '../lib/observable'
import { isOlderCopy } from '../lib/older-copy'
import { relayAddressToSave } from '../lib/relay-address'
import { pref, type Pref } from '../lib/prefs'
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
}

async function request<T>(method: string, path: string, body?: unknown, signed = false): Promise<T> {
  const res = await window.codync.cloud.request(method, path, body, signed)
  if (!res.ok) throw new CloudError(res.error.status, res.error.code, res.error.message)
  return res.value as T
}

/** Host identities whose cloud default has been applied (or found already on). */
const cloudDefaultApplied = pref<string[]>('cloudDefaultApplied', [])
const userPrefs = new Map<string, Pref<string[]>>()
/** A list kept for each user; one pref each, as each one listens for changes. */
function userPref(name: string, userId: string) {
  const key = `${name}.${userId}`
  let list = userPrefs.get(key)
  if (!list) userPrefs.set(key, (list = pref<string[]>(key, [])))
  return list
}
/** Adds `id` to a list pref unless it's there. */
function include(list: Pref<string[]>, id: string) {
  if (!list.get().includes(id)) list.set([...list.get(), id])
}
/** The identities a user took out of their account here. */
const keptOut = (userId: string) => userPref('keptOutOfAccount', userId)
/**
 * The identities of this computer seen in a user's account: a later key reset cleans them up, and
 * one that leaves the account (a phone's "Remove from account" too) isn't added back on its own.
 */
const claimedIds = (userId: string) => userPref('claimedComputerIds', userId)
/** `claimedComputerIds` from before it was kept per user: its ids go to the first user signed in here. */
function moveClaimsToUser(userId: string) {
  try {
    const raw = localStorage.getItem('claimedComputerIds')
    if (raw === null) return
    localStorage.removeItem('claimedComputerIds')
    for (const id of JSON.parse(raw) as string[]) include(claimedIds(userId), id)
  } catch {}
}

/**
 * The signed-in account's side of this computer (mirrors kit's `CloudClient`): registering this device, the account's computers, claiming this
 * computer's host, and turning its cloud on by default.
 */
export class CloudModel extends Observable {
  computers: CloudComputer[] = []
  private registeredFor: string | null = null
  /** This app's computers (their ids) when the account's computers were last listed. */
  private listedWith: string | null = null
  /** Bumped by each list request: only the latest one's answer is kept. */
  private listVersion = 0
  /** The latest list request: an earlier one it overtook answers with it. */
  private listing: Promise<boolean> = Promise.resolve(false)
  private enablingCloud = false
  private autoClaiming = false

  constructor(private app: AppModel) {
    super()
    account.subscribe(() => this.tick())
    app.subscribe(() => this.tick())
  }

  /** What the user started shows its error; the app's own background calls (`quiet`) only log it. */
  private fail(error: unknown, quiet = false) {
    if (quiet) console.warn('Codync cloud:', error)
    else this.app.setError(error instanceof Error ? error.message : String(error))
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
    const managed = this.app.managedStores.map((s) => s.computer.id).sort().join(' ')
    if (this.registeredFor !== user) {
      this.registeredFor = user
      this.listedWith = managed
      moveClaimsToUser(user)
      // Signed in: make this computer known to the account, then list its computers.
      void (async () => {
        try {
          await request('POST', 'v1/devices', { name: await deviceName(), platform: { darwin: 'macos', linux: 'linux', win32: 'windows' }[window.codync.platform] }, true)
        } catch (error) {
          this.fail(error, true)
        }
        await this.refresh(true)
      })()
    } else if (this.listedWith !== managed) {
      // A computer came or went here (a host reset to a new identity, an uninstalled host, an SSH
      // computer): listed again, one that left shows how it is now, not as the last list had it.
      this.listedWith = managed
      void this.refresh(true)
    }
    this.applyCloudDefault()
    this.autoClaim()
  }

  /**
   * Lists the account's computers; `quiet` when the app does it on its own. True when a list at least
   * as new as this call's is shown: a newer request that overtook this one answers for it.
   */
  refresh(quiet = false): Promise<boolean> {
    const user = this.userId
    if (!user) return Promise.resolve(false)
    return (this.listing = this.list(user, ++this.listVersion, quiet))
  }

  private async list(user: string, version: number, quiet: boolean): Promise<boolean> {
    try {
      const res = await request<{ computers: CloudComputer[] }>('GET', 'v1/computers', undefined, true)
      // Signed out or into another account while it was on its way.
      if (this.userId !== user) return false
      // A newer list was asked for while this one was on its way (after a removal): that one is shown.
      if (version !== this.listVersion) return this.listing
      this.computers = res.computers
      this.changed()
      return true
    } catch (error) {
      if (this.userId !== user) return false
      if (version !== this.listVersion) return this.listing
      this.fail(error, quiet)
      return false
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
      .catch((error: unknown) => this.fail(error, true))
      .finally(() => (this.enablingCloud = false))
  }

  /** Signed in, this computer joins the account on its own (`mayAutoClaim`). */
  private autoClaim() {
    const user = this.userId
    const store = this.app.local
    if (user && store && store.cloud?.owner?.userId === user) include(claimedIds(user), store.computer.id)
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
    const user = this.userId
    const store = this.app.local
    if (!user || !store) return null
    const host = { id: store.computer.id, online: store.connection.kind === 'online', cloud: store.cloud }
    return mayAutoClaim(host, keptOut(user).get(), claimedIds(user).get()) ? store : null
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
      // Only against a list that came back: one that didn't can't tell which earlier identities are gone.
      if ((await this.refresh(true)) && store === this.app.local) await this.forgetEarlierIdentities(user, store.computer.id)
      return true
    } catch (error) {
      this.fail(error, quiet)
      return false
    }
  }

  /**
   * This computer's host got new keys (a reset data folder, a reinstall): the identities claimed
   * before are dead copies of it, so they leave the account instead of showing up twice. One still
   * online, or that couldn't be removed, stays listed for the next time.
   */
  private async forgetEarlierIdentities(user: string, current: string) {
    const seen = claimedIds(user)
    const kept = [current]
    for (const id of seen.get()) {
      const entry = this.computers.find((c) => c.computerId === id)
      if (id === current || !entry) continue
      if (entry.online || !(await this.removeFromAccount(id, true))) kept.push(id)
    }
    seen.set(kept)
  }

  async unclaim(store: BotStore) {
    const user = this.userId
    if (user) include(keptOut(user), store.computer.id)
    try {
      await (await store.ready()).call('unclaim', {}, 30_000)
      await this.refresh(true)
    } catch (error) {
      this.fail(error)
    }
  }

  /** Points this computer's relay at `raw`, or clears it when `raw` is blank. False when the host refuses. */
  async setCloudUrl(store: BotStore, raw: string): Promise<boolean> {
    const next = relayAddressToSave(raw, store.cloud?.url)
    return next === null || (await this.saveRelay(store, { url: next }))
  }

  /** Saves the private relay's registration token for the address in use. Blank leaves it as it is. */
  async setRelayToken(store: BotStore, token: string): Promise<void> {
    const url = store.cloud?.url
    if (url && token.trim()) await this.saveRelay(store, { url, token: token.trim() })
  }

  /** The host's `setCloudUrl`. The relay restarts, same as turning the cloud on or off. */
  private async saveRelay(store: BotStore, body: { url: string; token?: string }): Promise<boolean> {
    try {
      await (await store.ready()).call('setCloudUrl', body, 30_000)
      return true
    } catch (error) {
      this.fail(error)
      return false
    }
  }

  async setCloud(store: BotStore, enabled: boolean) {
    include(cloudDefaultApplied, store.computer.id)
    try {
      if (enabled) await this.enableCloud(store)
      else await (await store.ready()).call<CloudStatus>('setCloud', { enabled: false }, 30_000)
    } catch (error) {
      this.fail(error)
    }
  }

  /** Whether the user's cloud assistant may message this computer's bots (the host's `cloud` event follows). */
  async setAssistants(store: BotStore, enabled: boolean) {
    try {
      await (await store.ready()).call<CloudStatus>('setAssistants', { enabled })
    } catch (error) {
      this.fail(error)
    }
  }

  /** The host's cloud comes with its build; this only turns it on. */
  private async enableCloud(store: BotStore) {
    return (await store.ready()).call<CloudStatus>('setCloud', { enabled: true }, 30_000)
  }

  async removeFromAccount(computerId: string, quiet = false) {
    try {
      await request('DELETE', `v1/computers/${computerId}`)
      await this.refresh(true)
      return true
    } catch (error) {
      this.fail(error, quiet)
      return false
    }
  }

  isOlderCopy(c: CloudComputer) {
    return isOlderCopy(c, this.computers, this.app.computers)
  }
}

/** This computer's name for the account's device list. */
async function deviceName() {
  return window.codync.computerName || { darwin: 'Mac', linux: 'Linux', win32: 'Windows' }[window.codync.platform]
}
