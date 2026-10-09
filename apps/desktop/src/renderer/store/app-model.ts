import type { HostSnapshot } from '@shared/ipc'
import type { AccessRequest, Bot, Computer } from '@shared/models'
import { LoopbackTransport } from '../client/host-client'
import { Observable } from '../lib/observable'
import { account } from './account'
import { BotStore } from './bot-store'
import { CloudModel } from './cloud'
import { DeviceComputerOrder } from './computer-order'
import { ComputerAccess } from './computer-access'

/** A bot on a computer: bot ids are only unique per computer. */
export interface BotRef {
  computerId: string
  botId: string
}

export const refKey = (r: BotRef) => `${r.computerId}/${r.botId}`
export const parseRef = (key: string): BotRef | null => {
  const at = key.indexOf('/')
  return at < 0 ? null : { computerId: key.slice(0, at), botId: key.slice(at + 1) }
}
export const sameRef = (a: BotRef | null, b: BotRef | null) => !!a && !!b && a.computerId === b.computerId && a.botId === b.botId

export interface RosterItem {
  ref: BotRef
  bot: Bot
  computer: Computer
  store: BotStore
}

/** A device asking a computer this app manages for access. */
export interface Approval {
  id: string
  store: BotStore
  request: AccessRequest
}

/**
 * The window's model: this computer's own host (from the main process), one `BotStore` per
 * computer, the selection, and what the menu bar shows. Ports the Mac app's `HostController`
 * (renderer side) and kit's `AccountStore`.
 */
export class AppModel extends Observable {
  host: HostSnapshot = { state: { kind: 'starting' }, local: null, dev: false, logPath: '' }
  computers: Computer[] = []
  stores = new Map<string, BotStore>()
  selection: BotRef | null = null
  lastError: string | null = null
  /** Codync Screen waits for the user to allow it in System Settings → Login Items. */
  screenNeedsApproval = false
  screenError: string | null = null
  private screenSynced = false
  private opened = false
  /** Whether the account was signed in when last seen; null until its state first arrives. */
  private signedIn: boolean | null = null
  /** Approvals closed with "later"; they come back when the request changes (its code arrives). */
  private deferred = new Set<string>()
  private storeSubscriptions = new Map<string, () => void>()

  /** The signed-in account's side of this computer (claims, cloud default, account computers). */
  readonly computerAccess: ComputerAccess
  readonly computerOrder: DeviceComputerOrder
  readonly cloud: CloudModel

  constructor(private mirrors = true) {
    super()
    this.computerAccess = new ComputerAccess(this)
    this.computerOrder = new DeviceComputerOrder()
    this.cloud = new CloudModel(this)
    account.subscribe(() => this.noteAccount())
  }

  setError(message: string | null) {
    this.lastError = message
    this.changed()
  }

  async start() {
    window.codync.host.onChange((s) => this.setHost(s))
    this.setHost(await window.codync.host.snapshot())
  }

  private setHost(snapshot: HostSnapshot) {
    const old = this.host.local
    this.host = snapshot
    const local = snapshot.local
    if (!this.mirrors) return this.changed()
    if (local && (old?.computerId !== local.computerId || old.token !== local.token || !this.stores.has(local.computerId))) {
      if (old && old.computerId !== local.computerId) this.detach(old.computerId)
      const cached = this.computers.find((c) => c.id === local.computerId)
      const computer: Computer = cached ?? { id: local.computerId, name: 'This computer', signKey: '', urls: [] }
      const store = new BotStore(computer, true, () => new LoopbackTransport(local.baseURL, local.token))
      this.attach(computer, store)
      store.subscribe(() => {
        this.syncScreen(store)
        this.noteOpened(store)
      })
    } else if (!local && old) {
      this.detach(old.computerId)
    }
    this.changed()
  }

  /** The computer's own host store. */
  get local(): BotStore | null {
    return this.host.local ? (this.stores.get(this.host.local.computerId) ?? null) : null
  }

  attach(computer: Computer, store: BotStore) {
    this.detach(computer.id)
    this.computers = [...this.computers.filter((c) => c.id !== computer.id), computer]
    this.stores.set(computer.id, store)
    store.onComputerChanged = (c) => {
      this.computers = this.computers.map((x) => (x.id === c.id ? c : x))
      this.changed()
    }
    store.onRosterChanged = () => this.changed()
    // Kit views select a new bot on their own store; that becomes the window's selection.
    this.storeSubscriptions.set(computer.id, store.subscribe(() => {
      if (store.selection) {
        const botId = store.selection
        store.selection = null
        this.select({ computerId: computer.id, botId })
      }
    }))
    store.start()
    this.changed()
  }

  detach(id: string) {
    this.storeSubscriptions.get(id)?.()
    this.storeSubscriptions.delete(id)
    this.stores.get(id)?.retire()
    this.stores.delete(id)
    this.computers = this.computers.filter((c) => c.id !== id)
    if (this.selection?.computerId === id) this.selection = null
    this.changed()
  }

  store(id: string) {
    return this.stores.get(id) ?? null
  }

  /** Every computer's visible bots, pinned first, then most recent activity. */
  get roster(): RosterItem[] {
    const items: RosterItem[] = []
    for (const computer of this.computers) {
      const store = this.stores.get(computer.id)
      if (!store) continue
      for (const bot of store.roster) items.push({ ref: { computerId: computer.id, botId: bot.id }, bot, computer: store.computer, store })
    }
    return items.sort((a, b) => (a.bot.pinned !== b.bot.pinned ? (a.bot.pinned ? -1 : 1) : b.bot.lastAt - a.bot.lastAt))
  }

  select(ref: BotRef | null) {
    this.selection = ref
    this.changed()
  }

  get selectedStore() {
    return this.selection ? this.store(this.selection.computerId) : null
  }

  /** Computers this app manages over loopback (itself and SSH tunnels): they take approvals and pairing. */
  get managedStores() {
    return [...this.stores.values()].filter((s) => s.isLoopback)
  }

  get approvals(): Approval[] {
    return this.managedStores.flatMap((store) =>
      store.accessRequests.map((request) => ({ id: `${store.computer.id}/${request.requestId}`, store, request })),
    )
  }

  /** The approval to show now: the first one not put off, or put off before its code arrived. */
  get currentApproval(): Approval | null {
    return this.approvals.find((a) => !this.deferred.has(deferKey(a))) ?? null
  }

  deferApproval(a: Approval) {
    this.deferred.add(deferKey(a))
    this.changed()
  }

  /** Brings every waiting request back (the menu's Review). */
  reviewApprovals() {
    this.deferred.clear()
    this.changed()
  }

  async decide(a: Approval, approve: boolean) {
    const client = await a.store.ready()
    await client.call('decideAccessRequest', { requestId: a.request.requestId, approve }, 30_000)
  }

  get needsAttention() {
    return this.roster.some((i) => i.bot.status === 'needsInput') || this.approvals.length > 0
  }

  get working() {
    return this.roster.filter((i) => i.bot.status === 'working' || i.bot.status === 'needsInput').length
  }

  get errorMessage(): string | null {
    if (this.lastError) return this.lastError
    for (const store of this.stores.values()) if (store.lastError) return store.lastError
    return null
  }

  /** Remote screen: starts/stops Codync Screen and tells the host (it only accepts this from the computer itself). */
  async setRemoteScreen(on: boolean) {
    const local = this.local
    if (!local) return
    try {
      this.screenNeedsApproval = (await window.codync.app.setScreenAgent(on)).needsApproval
      await local.setScreenEnabled(on)
      this.screenError = null
    } catch (error) {
      this.screenError = error instanceof Error ? error.message : String(error)
    }
    this.changed()
  }

  /** Once per launch, keeps the agent registered while the host has Remote screen on. */
  private syncScreen(store: BotStore) {
    if (this.screenSynced || !store.screen) return
    this.screenSynced = true
    void window.codync.app.syncScreenAgent(store.screen.enabled).then((r) => {
      this.screenNeedsApproval = r.needsApproval
      this.changed()
    }).catch((error: unknown) => {
      this.screenError = error instanceof Error ? error.message : String(error)
      this.changed()
    })
  }

  /** `app_opened` once per launch, when this computer's host first answers. */
  private noteOpened(store: BotStore) {
    if (this.opened || !store.hello) return
    this.opened = true
    store.track('app_opened')
  }

  /** Sign-in and sign-out, not the state restored at launch. */
  private noteAccount() {
    if (!account.isReady) return
    const now = account.isSignedIn
    if (this.signedIn !== null && now !== this.signedIn) this.local?.track(now ? 'signed_in' : 'signed_out')
    this.signedIn = now
  }

  clearErrors() {
    this.lastError = null
    for (const store of this.stores.values()) store.lastError = null
    this.changed()
  }
}

const deferKey = (a: Approval) => `${a.id}/${a.request.code ?? ''}`
