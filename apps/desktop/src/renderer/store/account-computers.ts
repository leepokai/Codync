import type { AccessFailure } from '@shared/analytics'
import { Observable } from '../lib/observable'
import { analytics } from './analytics'
import type { AppModel } from './app-model'
import { CloudError, type CloudComputer } from './cloud'

/** An access request this app is waiting on: `code` is null until the computer shows it too. */
type Asking = { code: string | null }

/**
 * Requests for access to the account's other computers, as the phones ask: the computer and this app show the
 * same 6-digit code, and its bots open (`useCloudAttachments`) once it's approved there.
 */
export class AccountComputers extends Observable {
  asking = new Map<string, Asking>()
  private app: AppModel

  constructor(app: AppModel) {
    super()
    this.app = app
  }

  /** Asks `computer` to let this app in. */
  async ask(computer: CloudComputer) {
    const id = computer.computerId
    if (this.asking.has(id)) return
    const asking: Asking = { code: null }
    this.set(id, asking)
    const result = await window.codync.cloud.requestAccess(computer, (code) => {
      // Withdrawn while the computer's code was on its way.
      if (this.asking.get(id) !== asking) return
      asking.code = code
      analytics.track({ event: 'access_requested' })
      this.changed()
    })
    // Withdrawn here: `cancel` already counted it.
    if (this.asking.get(id) !== asking) return
    this.set(id, null)
    if (!result.ok) return this.app.setError(new CloudError(result.error.status, result.error.code, result.error.message).message)
    if (result.value.status === 'approved') return void this.app.cloud.refresh()
    const reason: AccessFailure = result.value.status === 'denied' ? 'denied' : result.value.status === 'cancelled' ? 'cancelled' : 'expired'
    analytics.track({ event: 'access_request_failed', reason })
    this.app.setError(reason === 'denied' ? 'The computer declined the request.' : new CloudError(410, 'requestExpired', null).message)
  }

  /** Withdraws the request: the computer stops showing it. */
  cancel(computerId: string) {
    if (this.asking.get(computerId)?.code) analytics.track({ event: 'access_request_failed', reason: 'cancelled' })
    this.set(computerId, null)
    window.codync.cloud.cancelAccess(computerId)
  }

  private set(computerId: string, asking: Asking | null) {
    if (asking) this.asking.set(computerId, asking)
    else this.asking.delete(computerId)
    this.changed()
  }
}
