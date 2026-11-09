import {
  actionMachine, canCreate, canRemoveKey, cloudCreateOutcome, cloudKeyOutcome, errorPlace, isPreparing, justClaimed,
  type CloudComputerAction, type CloudCredential, type CloudMachine, type CloudProvider,
} from '../lib/cloud-computer'
import { Observable } from '../lib/observable'
import { account } from './account'
import { analytics } from './analytics'
import type { AppModel } from './app-model'
import { request } from './cloud'

/** Asked again this often while a new cloud computer claims itself (about 10 s), slower if it takes long. */
const PREPARING_POLL_MS = 3_000
const PREPARING_POLL_MAX_MS = 30_000

/**
 * Computers → Codync Cloud: the providers the cloud can run a computer on with the keys the user gave them, the
 * account's cloud computers (`/v1/cloud-computers`, oldest first), creating, renaming and deleting them or their
 * snapshots. Once claimed each is an account computer like any other (`CloudModel.computers`).
 */
export class CloudComputerModel extends Observable {
  machines: CloudMachine[] = []
  /** How many the account may have. */
  limit = 0
  /** Every provider the cloud offers, each with the account's key for it, if any. */
  providers: CloudProvider[] = []
  /** The first answer came back for this account. */
  loaded = false
  /** This account may create them (the cloud can seal keys: CLOUD_SECRET_KEYS). */
  available = false
  busy: CloudComputerAction | null = null
  error: string | null = null
  /** The action `error` came from (null: the lookup). */
  private failed: CloudComputerAction | null = null
  private user = account.user?.userId ?? null
  private polling = false
  /** Bumped by every change made here (create, rename, delete): a lookup sent before one is stale when it comes back. */
  private generation = 0

  constructor(private app: AppModel) {
    super()
    account.subscribe(() => this.noteAccount())
  }

  /** Another account (or none): nothing of the last one stays. */
  private noteAccount() {
    const user = account.user?.userId ?? null
    if (user === this.user) return
    this.user = user
    this.machines = []
    this.limit = 0
    this.providers = []
    this.loaded = false
    this.available = false
    this.busy = null
    this.error = null
    this.changed()
  }

  async refresh() {
    await this.run(null, async (current) => {
      const sent = this.generation
      const res = await request<{ cloudComputers: CloudMachine[]; available: boolean; limit: number; providers: CloudProvider[] }>('GET', 'v1/cloud-computers')
      if (!current() || sent !== this.generation) return
      this.available = res.available
      this.limit = res.limit
      this.providers = res.providers
      this.show(res.cloudComputers)
    })
  }

  get canCreate() {
    return canCreate(this.available, this.busy !== null, this.machines.length, this.limit)
  }

  /** The computer `id`'s own action is running: a spinner in its `⋯` menu's place. */
  isBusy(id: string) {
    return actionMachine(this.busy) === id
  }

  /** `error` where it shows: the card, the New cloud computer sheet, or under a provider's key (`key:boat`). */
  errorAt(place: ReturnType<typeof errorPlace>) {
    return this.error && errorPlace(this.failed) === place ? this.error : null
  }

  /** A key removal's failure, whichever provider's: New cloud computer shows one asked from a row's `⋯` there. */
  get keyRemovalError() {
    return this.failed?.startsWith('removeKey:') ? this.error : null
  }

  /** Forgets an earlier failure shown at `place` (a sheet opened afresh). */
  clearError(place: ReturnType<typeof errorPlace>) {
    if (!this.errorAt(place)) return
    this.error = null
    this.failed = null
    this.changed()
  }

  /**
   * Gives the cloud a key to the user's own account with `provider`; the cloud checks it there, keeps it sealed and
   * answers what to fix if it won't. True once kept.
   */
  async saveKey(provider: string, key: string): Promise<boolean> {
    let saved = false
    await this.run(`saveKey:${provider}`, async (current) => {
      try {
        const res = await request<{ credential: CloudCredential }>('PUT', `v1/cloud-credentials/${provider}`, { key: key.trim() })
        analytics.track({ event: 'cloud_key_saved', outcome: 'saved', provider })
        if (!current()) return
        this.setCredential(provider, res.credential)
        saved = true
      } catch (error) {
        analytics.track({ event: 'cloud_key_saved', outcome: cloudKeyOutcome(error), provider })
        throw error
      }
    })
    return saved
  }

  /** Opens `provider`'s free plan in the browser, counted by its provider. */
  openOffer(provider: string, url: string) {
    analytics.track({ event: 'cloud_offer_opened', provider })
    window.codync.app.openExternal(url)
  }

  canRemoveKey(provider: string) {
    return canRemoveKey(this.machines, provider)
  }

  /** Refused (409) while a cloud computer runs on that provider. */
  removeKey(provider: string) {
    return this.run(`removeKey:${provider}`, async (current) => {
      await request('DELETE', `v1/cloud-credentials/${provider}`)
      if (current()) this.setCredential(provider, null)
    })
  }

  private setCredential(provider: string, credential: CloudCredential | null) {
    this.providers = this.providers.map((p) => (p.provider === provider ? { ...p, credential } : p))
  }

  /** Another cloud computer named `name` on `provider`, one with a usable key. True once the cloud made it. */
  async create(provider: string, name: string): Promise<boolean> {
    let created = false
    await this.run('create', async (current) => {
      try {
        const res = await request<{ cloudComputer: CloudMachine }>('POST', 'v1/cloud-computers', { provider, name })
        analytics.track({ event: 'cloud_computer_created', outcome: 'created', provider })
        if (!current()) return
        this.generation++
        this.show([...this.machines, res.cloudComputer])
        created = true
      } catch (error) {
        analytics.track({ event: 'cloud_computer_created', outcome: cloudCreateOutcome(error), provider })
        throw error
      }
    })
    return created
  }

  /** Its account computer and its host take the name too; shown here at once (the host, asleep perhaps, says so later). */
  rename(id: string, name: string) {
    return this.run(`rename:${id}`, async (current) => {
      const res = await request<{ cloudComputer: CloudMachine }>('PATCH', `v1/cloud-computers/${id}`, { name }).catch((error: unknown) => {
        analytics.track({ event: 'cloud_computer_renamed', outcome: 'failed' })
        throw error
      })
      analytics.track({ event: 'cloud_computer_renamed', outcome: 'renamed' })
      if (!current()) return
      this.generation++
      this.show(this.machines.map((m) => (m.cloudComputerId === id ? res.cloudComputer : m)))
      if (res.cloudComputer.computerId) this.app.store(res.cloudComputer.computerId)?.setHostName(res.cloudComputer.name)
      void this.app.cloud.refresh(true)
    })
  }

  /** Erases the earlier saved states and the backup; the computer stays. Refused (409) while it sleeps. */
  deleteSnapshots(id: string) {
    return this.run(`deleteSnapshots:${id}`, async () => {
      await request('DELETE', `v1/cloud-computers/${id}/snapshots`)
      await this.refresh()
    })
  }

  /** Erases the machine, its snapshots and its backup, and takes its computer out of the account. */
  delete(id: string) {
    return this.run(`delete:${id}`, async (current) => {
      await request('DELETE', `v1/cloud-computers/${id}`)
      if (!current()) return
      this.generation++
      this.show(this.machines.filter((m) => m.cloudComputerId !== id))
      void this.app.cloud.refresh(true)
    })
  }

  /** Runs one request for the account signed in now; `current` is false once another account (or none) is. */
  private async run(action: CloudComputerAction | null, work: (current: () => boolean) => Promise<void>) {
    const user = this.user
    const current = () => this.user === user
    if (!user || (action && this.busy)) return
    if (action) {
      this.busy = action
      this.error = null
      this.failed = null
      this.changed()
    }
    try {
      await work(current)
      if (current() && !action) this.error = null
    } catch (error) {
      if (current()) {
        this.error = error instanceof Error ? error.message : String(error)
        this.failed = action
      }
    } finally {
      if (current()) {
        if (action) this.busy = null
        this.changed()
      }
    }
  }

  /** Settings shows Codync Cloud only to an account that may have them, or has some. */
  get shown() {
    return this.loaded && (this.available || this.machines.length > 0)
  }

  private show(machines: CloudMachine[]) {
    const claimed = justClaimed(this.machines, machines)
    this.machines = machines
    this.loaded = true
    // A computer just joined the account: listed, it shows up with the others.
    if (claimed) void this.app.cloud.refresh(true)
    if (machines.some(isPreparing)) void this.pollWhilePreparing()
  }

  private async pollWhilePreparing() {
    if (this.polling) return
    this.polling = true
    let delay = PREPARING_POLL_MS
    while (this.machines.some(isPreparing)) {
      await new Promise((r) => setTimeout(r, delay))
      delay = Math.min(delay * 2, PREPARING_POLL_MAX_MS)
      if (this.machines.some(isPreparing)) await this.refresh()
    }
    this.polling = false
  }
}
