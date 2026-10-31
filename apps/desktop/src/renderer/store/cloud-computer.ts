import { isPreparing, type CloudMachine } from '../lib/cloud-computer'
import { Observable } from '../lib/observable'
import { account } from './account'
import { request, type CloudModel } from './cloud'

export type CloudComputerAction = 'create' | 'deleteSnapshots' | 'delete'

/** Asked again this often while a new cloud computer claims itself (about 10 s), slower if it takes long. */
const PREPARING_POLL_MS = 3_000
const PREPARING_POLL_MAX_MS = 30_000

/**
 * Settings → Codync Cloud: the account's cloud computer (`/v1/cloud-computers`), creating it and deleting
 * it or its snapshots. Once claimed it is an account computer like any other (`CloudModel.computers`).
 */
export class CloudComputerModel extends Observable {
  machine: CloudMachine | null = null
  /** The first answer came back for this account. */
  loaded = false
  /** This account may create one (an allowlist while payments are set aside). */
  available = false
  busy: CloudComputerAction | null = null
  error: string | null = null
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
    this.loaded = false
    this.available = false
    this.busy = null
    this.error = null
    this.changed()
  }

  async refresh() {
    await this.run(null, async (current) => {
      const res = await request<{ cloudComputer: CloudMachine | null; available?: boolean }>('GET', 'v1/cloud-computers')
      if (!current()) return
      this.available = res.available ?? false
      this.show(res.cloudComputer)
    })
  }

  create() {
    return this.run('create', async (current) => {
      const res = await request<{ cloudComputer: CloudMachine }>('POST', 'v1/cloud-computers', {})
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
      this.changed()
    }
    try {
      await work(current)
      if (current() && !action) this.error = null
    } catch (error) {
      if (current()) this.error = error instanceof Error ? error.message : String(error)
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
