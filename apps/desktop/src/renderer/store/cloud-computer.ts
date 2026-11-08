import { cloudKeyOutcome, connectedProviders, isPreparing, type CloudCredential, type CloudMachine, type CloudProvider } from '../lib/cloud-computer'
import { Observable } from '../lib/observable'
import { account } from './account'
import { analytics } from './analytics'
import { request, type CloudModel } from './cloud'

/** One action at a time; a key's carries its provider (`saveKey:boat`). */
export type CloudComputerAction = 'create' | 'deleteSnapshots' | 'delete' | `saveKey:${string}` | `removeKey:${string}`

/** Asked again this often while a new cloud computer claims itself (about 10 s), slower if it takes long. */
const PREPARING_POLL_MS = 3_000
const PREPARING_POLL_MAX_MS = 30_000

/**
 * Computers → Codync Cloud: the providers the cloud can run a computer on with the keys the user gave them, the
 * account's cloud computer (`/v1/cloud-computers`), creating it and deleting it or its snapshots. Once claimed it is an
 * account computer like any other (`CloudModel.computers`).
 */
export class CloudComputerModel extends Observable {
  machine: CloudMachine | null = null
  /** Every provider the cloud offers, each with the account's key for it, if any. */
  providers: CloudProvider[] = []
  /** The first answer came back for this account. */
  loaded = false
  /** This account may create one (an allowlist of user ids). */
  available = false
  busy: CloudComputerAction | null = null
  error: string | null = null
  /** The action `error` came from (null: the lookup). */
  private failed: CloudComputerAction | null = null
  private user = account.user?.userId ?? null
  private polling = false

  constructor(private cloud: CloudModel) {
    super()
    account.subscribe(() => this.noteAccount())
  }

  /** Another account (or none): nothing of the last one stays. */
  private noteAccount() {
    const user = account.user?.userId ?? null
    if (user === this.user) return
    this.user = user
    this.machine = null
    this.providers = []
    this.loaded = false
    this.available = false
    this.busy = null
    this.error = null
    this.changed()
  }

  async refresh() {
    await this.run(null, async (current) => {
      const res = await request<{ cloudComputer: CloudMachine | null; available?: boolean; providers?: CloudProvider[] }>('GET', 'v1/cloud-computers')
      if (!current()) return
      this.available = res.available ?? false
      this.providers = res.providers ?? []
      this.show(res.cloudComputer)
    })
  }

  /** Where a computer can be created: the providers the account gave a key. Creating says what to fix if one still
   * turns its key down. */
  get connected() {
    return connectedProviders(this.providers)
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

  /**
   * `error` where the providers sheet shows it: under `provider`'s key when saving or removing that key failed, else
   * (null) below every provider.
   */
  errorFor(provider: string | null) {
    const keyOf = this.failed?.match(/^(?:saveKey|removeKey):(.+)$/)?.[1] ?? null
    return keyOf === provider ? this.error : null
  }

  /** The computer's key stays: without it the cloud couldn't put the computer to sleep. */
  canRemoveKey(provider: string) {
    return this.machine?.provider !== provider
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

  /** On `provider`, one of `connected`. */
  create(provider: string) {
    return this.run('create', async (current) => {
      const res = await request<{ cloudComputer: CloudMachine }>('POST', 'v1/cloud-computers', { provider })
      if (current()) this.show(res.cloudComputer)
    })
  }

  /** Erases the earlier saved states and the backup; the computer stays. Refused (409) while it sleeps. */
  deleteSnapshots() {
    return this.run('deleteSnapshots', async () => {
      await request('DELETE', 'v1/cloud-computers/snapshots')
      await this.refresh()
    })
  }

  /** Erases the machine, its snapshots and its backup, and takes its computer out of the account. */
  delete() {
    return this.run('delete', async (current) => {
      await request('DELETE', 'v1/cloud-computers')
      if (!current()) return
      this.show(null)
      void this.cloud.refresh(true)
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

  /** Settings shows Codync Cloud only to an account that may have one, or has one. */
  get shown() {
    return this.loaded && (this.available || this.machine !== null)
  }

  private show(machine: CloudMachine | null) {
    const claimed = this.machine?.computerId === null && machine?.computerId
    this.machine = machine
    this.loaded = true
    // Its computer just joined the account: listed, it shows up with the others.
    if (claimed) void this.cloud.refresh(true)
    if (isPreparing(machine)) void this.pollWhilePreparing()
  }

  private async pollWhilePreparing() {
    if (this.polling) return
    this.polling = true
    let delay = PREPARING_POLL_MS
    while (isPreparing(this.machine)) {
      await new Promise((r) => setTimeout(r, delay))
      delay = Math.min(delay * 2, PREPARING_POLL_MAX_MS)
      if (isPreparing(this.machine)) await this.refresh()
    }
    this.polling = false
  }
}
