import { Observable } from '../lib/observable'
import { pref, prefs, type Pref } from '../lib/prefs'
import { moveInOrder } from '../lib/computer-roster'
import { account } from './account'

/** This device's order, kept separately for each account and cloud environment. */
export class DeviceComputerOrder extends Observable {
  private scope: string | null = null
  private saved: Pref<{ ids: string[] }> | null = null
  private accounts = new Map<string, Pref<{ ids: string[] }>>()
  private unsubscribe: (() => void) | undefined

  constructor() {
    super()
    account.subscribe(() => this.updateAccount())
    prefs.computerOrder.subscribe(() => { if (!this.saved) this.changed() })
    this.updateAccount()
  }

  get ids() { return this.saved?.get().ids ?? prefs.computerOrder.get() }

  move(id: string, target: string, computers: string[]) {
    const current = [...new Set([...this.ids, ...computers])]
    const next = moveInOrder(current, id, target)
    if (next === current) return
    if (this.saved) this.saved.set({ ids: next })
    else prefs.computerOrder.set(next)
  }

  private updateAccount() {
    const user = account.user?.userId
    const scope = user ? `${account.cloudURL}/${user}` : null
    if (scope === this.scope) return
    this.unsubscribe?.()
    this.scope = scope
    this.saved = null
    if (scope) {
      const saved = this.accounts.get(scope) ?? pref<{ ids: string[] }>(`computerOrder.${scope}`, { ids: [] })
      this.accounts.set(scope, saved)
      this.saved = saved
      this.unsubscribe = saved.subscribe(() => this.changed())
    }
    this.changed()
  }
}
