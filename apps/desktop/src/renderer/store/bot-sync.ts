import { isChat, type AccessRequest, type CloudStatus, type Entry } from '@shared/models'
import { isOutsideLoadedWindow, snapshotFloors } from '@shared/mirror-window'
import { HostClient, HostError, type HostEvent } from '../client/host-client'
import { undoOnFailure } from '../lib/undo-on-failure'
import { BotMirror, type Connection } from './bot-mirror'
import { Artifacts } from './artifacts'
import type { CallCue } from '@shared/calls'

const REFRESH_TIMEOUT = 20_000
const DROP_GRACE = 5000
const INITIAL_GRACE = 1000
const ACTION_PATIENCE = 20_000
const MISMATCH_RECHECK = 30_000
/** The mismatch recheck while an update asked from here runs, so the notice leaves soon after. */
const UPDATE_RECHECK = 5_000

export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
const same = (a: Connection, b: Connection) => a.kind === b.kind && JSON.stringify(a) === JSON.stringify(b)

/** Each chat's entries that `keep` accepts; chats left empty go. */
function keptEntries(entries: Map<string, Entry[]>, keep: (e: Entry) => boolean) {
  const kept = new Map<string, Entry[]>()
  for (const [id, list] of entries) {
    const rest = list.filter(keep)
    if (rest.length) kept.set(id, rest)
  }
  return kept
}

/**
 * The link half of `BotStore`: the events stream (catch-up since `rev`, then live), connection
 * graces, waiting for the link before an action, and read acknowledgements.
 */
export class BotSync extends BotMirror {
  readonly artifacts = new Artifacts(() => this.changed())
  /** The calls in this app, by id: each hands what the computer tells its voice to its engine. */
  readonly callCues = new Map<string, (cue: CallCue) => void>()
  /** The newest cue each call heard (`CallCue.seq`), kept across engine switches: a replayed one isn't said again. */
  private readonly heardCues = new Map<string, number>()
  private finishRefresh: ((error?: Error) => void) | null = null
  private heldDrop: Connection | null = null
  private dropTimer: ReturnType<typeof setTimeout> | null = null
  private closeEvents: (() => void) | null = null
  private eventsLoop = 0
  private rewound = false
  /** The `since` the current events connection asked for, and the per-bot floors snapshotted at its `hello`. */
  private requestedSince = 0
  private floors = new Map<string, number>()
  private readingViews = new Map<string, string>()

  // MARK: lifecycle

  retire() {
    this.retired = true
    this.composerDrafts.retire()
    this.stopTransport()
    if (this.saveTimer) clearTimeout(this.saveTimer)
    this.onRosterChanged = null
    this.onComputerChanged = null
    this.client = null
    this.selection = null
  }

  start() {
    this.restartStream()
  }

  restartStream() {
    if (this.retired) return
    this.stopTransport()
    if (this.connection.kind !== 'online') this.setConnection({ kind: 'connecting' })
    const transport = this.makeTransport()
    this.client = new HostClient(transport)
    this.hostRoute = this.isLoopback ? 'loopback' : null
    this.runEvents(this.client)
  }

  /** Manual refresh waits for the new stream to answer, independently of the online grace period. */
  refreshStream(): Promise<void> {
    if (this.retired) return Promise.resolve()
    this.restartStream()
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => this.finishRefresh?.(new Error('Connection timed out')), REFRESH_TIMEOUT)
      this.finishRefresh = (error) => {
        clearTimeout(timer)
        this.finishRefresh = null
        if (error) reject(error)
        else resolve()
      }
    })
  }

  private stopTransport() {
    this.finishRefresh?.()
    this.eventsLoop++
    this.closeEvents?.()
    this.closeEvents = null
    if (this.dropTimer) clearTimeout(this.dropTimer)
    this.dropTimer = null
    this.heldDrop = null
  }

  /** Give initial connection failures a second to recover and online drops five seconds. */
  private setConnection(next: Connection) {
    const transient = next.kind === 'connecting' || next.kind === 'offline' || next.kind === 'computerOffline'
    const recovering = next.kind === 'online' && (this.connection.kind !== 'online' || this.heldDrop !== null)
    const initialFailure = this.connection.kind === 'connecting' && transient && next.kind !== 'connecting'
    if (transient && (this.connection.kind === 'online' || initialFailure)) {
      const grace = this.connection.kind === 'online' ? DROP_GRACE : INITIAL_GRACE
      this.heldDrop = next
      if (!this.dropTimer) {
        this.dropTimer = setTimeout(() => {
          this.dropTimer = null
          const held = this.heldDrop
          this.heldDrop = null
          if (held && !this.retired) {
            this.connection = held
            this.changed()
          }
        }, grace)
      }
      return
    }
    if (this.dropTimer) clearTimeout(this.dropTimer)
    this.dropTimer = null
    this.heldDrop = null
    if (!same(next, this.connection)) {
      this.connection = next
      this.changed()
    }
    if (recovering) for (const botId of new Set(this.readingViews.values())) this.markRead(botId)
  }

  /** Events (catch-up since `rev`, then live) for as long as the store lives. */
  private runEvents(client: HostClient) {
    const loop = ++this.eventsLoop
    let backoff = 1000
    const alive = () => loop === this.eventsLoop && !this.retired
    const attempt = async () => {
      if (!alive()) return
      try {
        await this.refreshHello(client, loop)
        if (!alive()) return
        if (this.mismatch) {
          this.finishRefresh?.()
          this.setConnection({ kind: 'online' })
          await sleep(this.hostUpdateOffer.kind === 'updating' ? UPDATE_RECHECK : MISMATCH_RECHECK)
          return void attempt()
        }
        if (this.isLoopback) {
          try {
            this.accessRequests = (await client.call<{ requests: AccessRequest[] }>('accessRequests')).requests
          } catch {}
          try {
            this.cloud = await client.call<CloudStatus>('cloudStatus')
          } catch {}
          this.changed()
        }
        if (!alive()) return
        await new Promise<void>((resolve, reject) => {
          this.requestedSince = this.rev
          this.floors = new Map()
          this.closeEvents = client.events(
            this.requestedSince,
            'desktop',
            (event) => {
              if (!alive()) return
              this.setConnection({ kind: 'online' })
              backoff = 1000
              this.apply(event)
              this.finishRefresh?.()
            },
            (error) => (error ? reject(error) : resolve()),
          )
        })
      } catch (error) {
        if (!alive()) return
        const message = error instanceof Error ? error.message : String(error)
        this.finishRefresh?.(new Error(message))
        this.setConnection({ kind: 'offline', message })
        if (error instanceof HostError && error.status === 401) return
      }
      if (!alive()) return
      await sleep(backoff)
      backoff = Math.min(backoff * 2, 20_000)
      void attempt()
    }
    void attempt()
  }

  /** Resubscribes from the current `rev` on the same link. */
  private restartEvents() {
    if (!this.client) return
    this.closeEvents?.()
    this.runEvents(this.client)
  }

  private async refreshHello(client: HostClient, loop: number) {
    const epoch = ++this.iconsEpoch
    const h = await client.hello()
    if (this.retired || loop !== this.eventsLoop || epoch !== this.iconsEpoch) return
    this.iconsRevision = -1
    this.hello = h
    this.applyIcons(h.icons ?? {}, h.iconsRevision)
    this.noteHostVersion({ version: h.version, minApp: h.minApp })
    if (this.isLoopback) this.updateComputer((c) => ({ ...c, name: h.name, device: h.device ?? c.device }))
    this.changed()
  }

  private noteHostVersion(next: { version: string; minApp?: string | null }) {
    const cached = this.cacheStamp !== null && this.cacheStamp !== `${this.appVersion}/${next.version}`
    if (cached || (this.hostVersion && this.hostVersion.version !== next.version)) {
      // A different host version or app build: data may carry new fields, so fetch it all again.
      this.refetchAll()
      this.rewound = false
      this.unreadable = false
      this.hostUpdate = null
    }
    this.cacheStamp = null
    this.hostVersion = next
  }

  private apply(event: HostEvent) {
    switch (event.type) {
      case 'icons':
        this.applyIcons(event.icons, event.iconsRevision)
        break
      case 'hello':
        if (this.hostId && this.hostId !== event.hostId) this.resetMirror()
        this.hostId = event.hostId
        if (event.icons) this.applyIcons(event.icons)
        this.usage = event.usage
        this.screen = event.screen
        this.network = event.network
        if (event.rev < this.rev) this.rev = 0
        this.floors = snapshotFloors(this.entries, this.rev === 0 ? 0 : this.requestedSince)
        break
      case 'bot':
        this.putHostBot(event.bot)
        this.bump(event.bot.rev)
        this.onRosterChanged?.()
        if (event.bot.unread > 0) this.acknowledgeVisible(event.bot.id)
        break
      case 'botDeleted':
        this.composerDrafts.removeBot(event.id)
        this.bots.delete(event.id)
        this.entries.delete(event.id)
        this.floors.delete(event.id)
        this.bump(event.rev)
        if (this.selection === event.id) this.selection = null
        this.onRosterChanged?.()
        break
      case 'entry':
        // Old entries re-stamped by the host (new rev) belong to history paging, not the live window.
        if (!this.isBelowFloor(event.entry)) this.upsert(event.entry)
        this.bump(event.entry.rev)
        this.acknowledgeVisible(event.entry.botId, event.entry)
        break
      case 'usage':
        this.usage = event.usage
        break
      case 'artifact':
        this.artifacts.upsert(event.artifact)
        break
      case 'screen':
        this.screen = event.screen
        break
      case 'accessRequests':
        this.accessRequests = event.requests
        break
      case 'cloud':
        this.cloud = event.cloud
        break
      case 'network':
        this.network = event.network
        break
      case 'call': {
        const { cue } = event
        const hear = this.callCues.get(cue.call)
        if (!hear || cue.seq <= (this.heardCues.get(cue.call) ?? 0)) break
        this.heardCues.set(cue.call, cue.seq)
        hear(cue)
        break
      }
      case 'resync':
        this.restartEvents()
        break
      case 'undecodable':
        // Never skip past data we couldn't read: rewind once and fetch everything again.
        if (!this.rewound) {
          this.rewound = true
          this.refetchAll()
          this.restartEvents()
        } else if (!this.unreadable) {
          this.unreadable = true
          if (this.mismatch) this.restartEvents()
        }
        break
    }
    this.changed()
    this.scheduleSave()
  }

  private resetMirror() {
    this.iconsRevision = -1
    this.applyIcons({})
    this.composerDrafts.clear()
    this.bots = new Map()
    this.entries = keptEntries(this.entries, (e) => e.id.startsWith('local-'))
    this.selection = null
    this.onRosterChanged?.()
    this.rev = 0
    this.hostId = null
    this.historyComplete = new Set()
    this.floors = new Map()
    this.screen = null
    this.saveCache()
  }

  /**
   * Fetches everything again from rev 0. That catch-up brings only each chat's newest entries, so the
   * main-chat ones held now go too: an older one kept would hide the gap from `loadOlder`, which pages
   * from the oldest one held. Messages still sending stay.
   */
  private refetchAll() {
    this.rev = 0
    this.entries = keptEntries(this.entries, (e) => e.id.startsWith('local-'))
    this.historyComplete = new Set()
  }

  private isBelowFloor(e: Entry) {
    const held = this.allEntries(e.botId).some((x) => x.id === e.id)
    return isOutsideLoadedWindow(this.floors.get(e.botId), held, e)
  }

  private bump(r: number) {
    this.rev = Math.max(this.rev, r)
  }

  // MARK: link

  protected get live(): HostClient | null {
    return this.connection.kind === 'online' && !this.heldDrop && !this.retired ? this.client : null
  }

  /** The client once the link is up. A reconnect in progress is waited out (headers show it). */
  async ready(deadline = Date.now() + ACTION_PATIENCE): Promise<HostClient> {
    let counted = false
    let retried = false
    try {
      while (!this.retired) {
        if (this.live) return this.live
        switch (this.connection.kind) {
          case 'online':
          case 'connecting':
            break
          case 'offline':
            if (!retried) {
              retried = true
              this.restartStream()
              break
            }
            throw HostError.unreachable()
          case 'unpaired':
            throw HostError.unreachable()
          case 'computerOffline':
            throw new HostError(0, 'Your computer is offline.')
          case 'unauthorized':
            throw new HostError(401, this.connection.message)
        }
        if (Date.now() >= deadline) break
        if (!counted) {
          counted = true
          this.waiting++
          this.changed()
        }
        await sleep(100)
      }
      throw HostError.unreachable()
    } finally {
      if (counted) {
        this.waiting--
        this.changed()
      }
    }
  }

  /** Runs `op` once the link is up; with `replay`, a call the link dropped under is tried again. */
  protected async withLink<T>(op: (c: HostClient) => Promise<T>, replay = false): Promise<T> {
    const deadline = Date.now() + ACTION_PATIENCE
    for (;;) {
      const client = await this.ready(deadline)
      try {
        return await op(client)
      } catch (error) {
        if (replay && error instanceof HostError && error.isTransient && Date.now() < deadline) {
          await sleep(500)
          continue
        }
        throw error
      }
    }
  }

  /** Fire and forget (resolving to whether it went through): what waiting and retrying couldn't settle reaches the dialog, after `undo` takes back what the action already showed. */
  protected perform(op: (c: HostClient) => Promise<unknown>, replay = false, undo?: () => void): Promise<boolean> {
    return undoOnFailure(this.withLink(op, replay), undo, (message) => {
      this.lastError = message
      this.changed()
    })
  }

  /** Views register while visible. Multiple windows may read the same conversation. */
  setReading(token: string, botId: string, active: boolean) {
    if (active) {
      this.readingViews.set(token, botId)
      if (this.connection.kind === 'online') this.markRead(botId)
    } else {
      this.readingViews.delete(token)
    }
  }

  private acknowledgeVisible(botId: string, entry?: Entry) {
    if (this.connection.kind !== 'online') return
    if (![...this.readingViews.values()].includes(botId) || (entry && !isChat(entry))) return
    this.markRead(botId)
  }

  markRead(botId: string) {
    void (async () => {
      if (!this.live) await sleep(INITIAL_GRACE)
      const client = this.live
      if (!client) return
      try {
        await client.call('markRead', { botId })
      } catch {
        // The visible scope is acknowledged again when the link recovers.
      }
    })()
  }

}
