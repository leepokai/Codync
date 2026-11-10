import type { ClaimFailure } from '@shared/analytics'
import type { CloudStatus, ClaimSignature } from '@shared/models'
import type { CloudPlacement } from '../lib/cloud-computer'
import { accountMismatch, mayJoinAfterInstall, mayOfferJoin, mayOfferMove, type AccountMismatch, type ClaimCandidate } from '../lib/join-offer'
import { Observable } from '../lib/observable'
import { isOlderCopy } from '../lib/older-copy'
import { relayAddressToSave } from '../lib/relay-address'
import { pref, type Pref } from '../lib/prefs'
import { account } from './account'
import { analytics } from './analytics'
import type { AppModel } from './app-model'
import type { BotStore } from './bot-store'
import { trNow } from '../lib/l10n.ts'

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
  /** Set for a cloud computer Codync runs (Codync Cloud). */
  cloud?: CloudPlacement | null
}

export class CloudError extends Error {
  constructor(
    readonly status: number,
    readonly code: string,
    message: string | null,
  ) {
    super(
      code === 'requestExpired' ? trNow('The request expired. Ask again.')
      : code === 'accountDeleted' ? trNow('This account was deleted.')
      : code === 'rateLimited' ? trNow('Too many requests. Try again in a little while.')
      : (message ?? `Codync cloud error (${code})`),
    )
  }
}

/** The Codync cloud's emergency floor: set once it refuses this app's version (`426 appUpdateRequired`). */
class AppFloor extends Observable {
  message: string | null = null

  reached(message: string | null) {
    if (this.message !== null) return
    this.message = message ?? trNow('Update Codync to keep using it.')
    this.changed()
  }
}

export const appFloor = new AppFloor()

/** `reach_account_answered`'s `error`: the category of a failed claim, never its message. */
const claimFailure = (error: unknown): ClaimFailure =>
  !(error instanceof CloudError) ? 'host' : error.code === 'unreachable' ? 'unreachable' : 'refused'

export async function request<T>(method: string, path: string, body?: unknown, signed = false): Promise<T> {
  const res = await window.codync.cloud.request(method, path, body, signed)
  if (res.ok) return res.value as T
  if (res.error.code === 'appUpdateRequired') appFloor.reached(res.error.message)
  throw new CloudError(res.error.status, res.error.code, res.error.message)
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
  /** A computer the user asked to add (Settings, an SSH install), waiting for the join prompt's answer. */
  private joinRequest: BotStore | null = null
  /** This computer is on its way from another account into this one (`moveHere`). */
  private moving = false
  /** A computer the user said yes to is on its way into this account (`acceptJoin`). */
  private joining = false

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
    this.noteClaimed()
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

  /** Remembers this computer's identity once it's seen in the account (`mayOfferJoin`). */
  private noteClaimed() {
    const user = this.userId
    const store = this.app.local
    if (user && store && store.cloud?.owner?.userId === user) include(claimedIds(user), store.computer.id)
  }

  /** What the join prompt asks about: a computer the user asked to add, or this one, once, while it's in no account. */
  get joinOffer(): BotStore | null {
    const user = this.userId
    if (!user || this.moving || this.joining) return null
    if (this.joinRequest) return this.joinRequest
    const store = this.app.local
    return store && mayOfferJoin(candidate(store), keptOut(user).get(), claimedIds(user).get()) ? store : null
  }

  /** Add to account, through the prompt that says which phones it removes. */
  requestJoin(store: BotStore) {
    this.joinRequest = store
    this.changed()
  }

  /**
   * Yes to `joinOffer`. The host hears of its account a moment later, so a computer that joined isn't asked about
   * again meanwhile; one that couldn't says why (`fail`), and Add to account tries again.
   */
  async acceptJoin(store: BotStore) {
    const user = this.userId
    if (this.joining || !user) return
    this.joinRequest = null
    this.joining = true
    this.changed()
    const joined = await this.claim(store)
    include(joined ? claimedIds(user) : keptOut(user), store.computer.id)
    this.joining = false
    this.changed()
    analytics.track({ event: 'join_offer_answered', outcome: joined ? 'added' : 'failed' })
  }

  /** The phones `store`'s computer would remove by joining the signed-in account. `null`: it couldn't say. A host before this method removes the same phones and can't name them. */
  joinWouldRemove = async (store: BotStore): Promise<string[] | null> => {
    const userId = this.userId
    if (!userId) return []
    try {
      return (await (await store.ready()).call<{ names: string[] }>('joinWouldRemove', { userId })).names
    } catch (error) {
      this.fail(error, true)
      return null
    }
  }

  /** The devices on `store`'s computer, which all go when it leaves its account. Empty when it can't say: the warning then stays general. */
  deviceNames = async (store: BotStore): Promise<string[]> => {
    try {
      return (await (await store.ready()).call<{ devices: { name: string }[] }>('devices')).devices.map((device) => device.name)
    } catch (error) {
      this.fail(error, true)
      return []
    }
  }

  /** No to `joinOffer`: phones reach it only over Wi-Fi or Tailscale, and it isn't offered again (Add to account still adds it). */
  declineJoin(store: BotStore) {
    const user = this.userId
    if (user) include(keptOut(user), store.computer.id)
    this.joinRequest = null
    this.changed()
    analytics.track({ event: 'join_offer_answered', outcome: 'kept_out' })
  }

  /** This computer, still in another account than the one signed in here: the user is asked to move it (`mayOfferMove`). */
  get moveOffer(): { store: BotStore; from: string | null } | null {
    const user = this.userId
    const store = this.app.local
    if (!user || !store || this.moving || !mayOfferMove(candidate(store), user, keptOut(user).get())) return null
    return { store, from: store.cloud?.owner?.email ?? null }
  }

  /** This computer and the account signed in here differ (`accountMismatch`): the sidebar's one line. */
  get mismatch(): AccountMismatch | null {
    const user = this.userId
    const store = this.app.local
    if (!user || !store || this.moving) return null
    return accountMismatch(candidate(store), user)
  }

  /** The sidebar line: asks again whether to move this computer here, even after "Keep it there". */
  reconsiderMove() {
    const user = this.userId
    const store = this.app.local
    if (!user || !store) return
    const out = keptOut(user)
    out.set(out.get().filter((x) => x !== store.computer.id))
    this.changed()
  }

  /** No to `moveOffer`: this computer stays in its account and isn't offered again (Remove, then Add to account, still moves it). */
  keepInOtherAccount(store: BotStore) {
    const user = this.userId
    if (user) include(keptOut(user), store.computer.id)
    this.changed()
  }

  /**
   * Yes to `moveOffer`: out of the other account, then into this one. Every device of that account loses
   * access, QR-paired ones included; out of an account the host turns the 6-digit check back on, so the new account's devices show
   * a code before getting in.
   */
  async moveHere(store: BotStore) {
    this.moving = true
    this.changed()
    try {
      await (await store.ready()).call('unclaim', {}, 30_000)
      await this.claim(store)
    } catch (error) {
      this.fail(error)
    } finally {
      this.moving = false
      this.changed()
    }
  }

  /** Asks to add an SSH computer just installed from here, once its host is up. */
  async joinAfterInstall(store: BotStore) {
    if (!this.userId) return
    try {
      if (mayJoinAfterInstall(await (await store.ready()).call<CloudStatus>('cloudStatus'))) this.requestJoin(store)
    } catch (error) {
      this.fail(error)
    }
  }

  /** §4.2 A: makes a computer this app manages part of the account; its cloud is turned on first. */
  async claim(store: BotStore, quiet = false): Promise<boolean> {
    try {
      await this.join(store)
      return true
    } catch (error) {
      this.fail(error, quiet)
      return false
    }
  }

  private async join(store: BotStore) {
    const user = this.userId
    if (!user || !store.client) throw new Error(trNow('Sign in first.'))
    const out = keptOut(user)
    out.set(out.get().filter((x) => x !== store.computer.id))
    const client = await store.ready()
    if (!store.cloud?.enabled) await this.enableCloud(store)
    const challenge = await request<{ claimId: string; nonce: string }>('POST', 'v1/claims', {})
    const signed = await client.call<ClaimSignature>('claimSign', { claimId: challenge.claimId, nonce: challenge.nonce, userId: user })
    await request('POST', `v1/claims/${challenge.claimId}/complete`, signed)
    // Only against a list that came back: one that didn't can't tell which earlier identities are gone.
    if ((await this.refresh(true)) && store === this.app.local) await this.forgetEarlierIdentities(user, store.computer.id)
  }

  /** Asked before Reach from anywhere turns on (`joinsAccountOnEnable`): Add to account, which turns it on too. */
  async addFromReach(store: BotStore) {
    try {
      await this.join(store)
      analytics.track({ event: 'reach_account_answered', outcome: 'added' })
    } catch (error) {
      this.fail(error)
      analytics.track({ event: 'reach_account_answered', outcome: 'failed', error: claimFailure(error) })
    }
  }

  /**
   * Asked before Reach from anywhere turns on: Use your own relay. It's on the Codync relay until its address is saved,
   * so it isn't offered the account meanwhile (`keptOut`; Add to account still adds it).
   */
  async useOwnRelay(store: BotStore) {
    const user = this.userId
    if (user) include(keptOut(user), store.computer.id)
    analytics.track({ event: 'reach_account_answered', outcome: 'own_relay' })
    await this.setCloud(store, true)
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

/** What the account rules (`join-offer`) read of a computer's host. */
function candidate(store: BotStore): ClaimCandidate {
  return { id: store.computer.id, online: store.connection.kind === 'online', cloud: store.cloud }
}

/** This computer's name for the account's device list. */
async function deviceName() {
  return window.codync.computerName || { darwin: 'Mac', linux: 'Linux', win32: 'Windows' }[window.codync.platform]
}
