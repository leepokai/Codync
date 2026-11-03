import type { AccessTicket } from '@shared/ipc'
import type { Computer } from '@shared/models'
import { RelayTransport } from '../client/host-client'
import { Observable } from '../lib/observable'
import { account } from './account'
import type { AppModel } from './app-model'
import { BotStore } from './bot-store'
import { CloudError, request, type CloudComputer } from './cloud'
import { relayPlan } from '../lib/relay-plan'

/** An access request this app is waiting on: null until the computer shows its code. */
type Asking = { ticket: AccessTicket | null }

/**
 * The account's other computers this app reaches through the account's relay, as the phones do: one it was
 * approved on opens as a store (its bots join the sidebar); one it wasn't asks for access with the 6-digit code.
 */
export class AccountComputers extends Observable {
  asking = new Map<string, Asking>()
  /** Ids of the relay stores attached here; one another route took over (SSH) is no longer ours to close. */
  private attached = new Set<string>()
  private syncing = false
  private app: AppModel

  constructor(app: AppModel) {
    super()
    this.app = app
  }

  /** Attaches every approved account computer the app doesn't reach otherwise, and detaches those no longer approved. */
  sync() {
    // Attaching notifies the app, which calls back here.
    if (this.syncing) return
    this.syncing = true
    try {
      const plan = relayPlan(this.app.cloud.computers, this.app.local?.computer.id ?? null, this.attached, (id) => this.app.store(id))
      this.attached = plan.kept
      for (const id of plan.detach) {
        this.app.detach(id)
        window.codync.remote.close(id)
      }
      for (const c of plan.attach) {
        const cached = this.app.computers.find((x) => x.id === c.computerId)
        const computer: Computer = { id: c.computerId, name: c.name, signKey: c.signKey, urls: [], cloud: account.cloudURL, device: c.device ?? null, color: cached?.color ?? null }
        this.attached.add(c.computerId)
        this.app.attach(computer, new BotStore(computer, false, () => new RelayTransport({ id: c.computerId, signKey: c.signKey })))
      }
    } finally {
      this.syncing = false
    }
  }

  /** Asks `computer` to let this app in; its bots open once it's approved there. */
  async ask(computer: CloudComputer) {
    const id = computer.computerId
    if (this.asking.has(id)) return
    const asking: Asking = { ticket: null }
    this.set(id, asking)
    try {
      const ticket = await window.codync.cloud.requestAccess({ id, signKey: computer.signKey })
      // Withdrawn while the computer was still answering: take it off its screen too.
      if (this.asking.get(id) !== asking) return withdraw(ticket)
      asking.ticket = ticket
      this.changed()
      if (await this.awaitDecision(id, ticket)) await this.app.cloud.refresh()
    } catch (error) {
      if (this.asking.get(id) === asking) this.app.setError(error instanceof Error ? error.message : (error as { message: string }).message)
    } finally {
      if (this.asking.get(id) === asking) this.set(id, null)
    }
  }

  /** Withdraws the request: the computer stops showing it. */
  cancel(computerId: string) {
    const ticket = this.asking.get(computerId)?.ticket
    this.set(computerId, null)
    if (ticket) withdraw(ticket)
  }

  /** True once approved; false when withdrawn here; throws when the computer declined or the request ran out. */
  private async awaitDecision(computerId: string, ticket: AccessTicket) {
    for (;;) {
      await new Promise((resolve) => setTimeout(resolve, 2_000))
      if (this.asking.get(computerId)?.ticket !== ticket) return false
      if (Date.now() >= ticket.expiresAt) throw new CloudError(410, 'requestExpired', null)
      const status = await request<{ status: string }>('GET', `v1/access-requests/${ticket.requestId}`, undefined, true).catch((error: unknown) => {
        if (error instanceof CloudError && error.status === 0) return { status: 'pending' }
        throw error
      })
      if (status.status === 'approved') return true
      if (status.status === 'denied') throw new Error('The computer declined the request.')
      if (status.status !== 'pending') throw new CloudError(410, 'requestExpired', null)
    }
  }

  private set(computerId: string, asking: Asking | null) {
    if (asking) this.asking.set(computerId, asking)
    else this.asking.delete(computerId)
    this.changed()
  }
}

function withdraw(ticket: AccessTicket) {
  void request('DELETE', `v1/access-requests/${ticket.requestId}`, undefined, true).catch(() => {})
}
