import {
  isWorking,
  needsInput,
  type AnalyticsEvent,
  type Bot,
  type BotDraft,
  type DirListing,
  type Entry,
  type GroupDraft,
  type Hello,
  type ScreenState,
  type Usage,
} from '@shared/models'
import { FileDownloads } from './file-downloads'
import { BotSync, sleep } from './bot-sync'

export { builtInBackendName, type Connection } from './bot-mirror'

/** A file picked in the composer, sent with the next message. */
export interface OutgoingFile {
  id: string
  name: string
  data: Uint8Array
}

/** Anything larger is refused by the computer. */
export const MAX_FILE_SIZE = 100 * 1024 * 1024

export const QUICK_REACTIONS = ['👍', '❤️', '😂', '🎉', '👀', '✅']

/**
 * One computer's mirror (bots, transcripts), kept by one events stream (catch-up since
 * `rev`, then live). Port of kit's `BotStore`.
 */
export class BotStore extends BotSync {
  readonly fileDownloads = new FileDownloads(() => this.changed())

  override retire() { this.fileDownloads.retire(); super.retire() }

  private outgoingFiles = new Map<string, OutgoingFile[]>()

  send(text: string, botId: string, thread: string | null = null, files: OutgoingFile[] = [], nonce: string = crypto.randomUUID()) {
    const trimmed = text.trim()
    if ((!trimmed && !files.length) || this.retired) return
    const data: Entry['data'] = { text: trimmed, status: 'sending', clientNonce: nonce }
    if (files.length) {
      data.attachments = files.map((f) => ({ id: f.id, name: f.name, size: f.data.length }))
      this.outgoingFiles.set(nonce, files)
    }
    this.upsert({
      id: `local-${nonce}`, seq: Number.MAX_SAFE_INTEGER, botId, threadId: thread, rev: 0, kind: 'user',
      turn: 0, data, createdAt: Date.now(), updatedAt: 0,
    })
    this.changed()
    void (async () => {
      try {
        // Safe to repeat: the computer skips a nonce it already has.
        const e = await this.withLink(async (client) => {
          let ids: string[] | null = null
          const pending = this.outgoingFiles.get(nonce)
          if (pending) {
            ids = []
            for (const file of pending) {
              const id = crypto.randomUUID()
              await client.upload(botId, id, file.name, file.data)
              cacheAttachment(id, file.data)
              ids.push(id)
            }
          }
          return client.send(botId, trimmed, nonce, thread, ids)
        }, true)
        this.outgoingFiles.delete(nonce)
        this.upsert(e)
        this.changed()
      } catch (error) {
        if (this.outgoingFiles.has(nonce)) this.lastError = `Couldn't send the files: ${error instanceof Error ? error.message : error}`
        this.markLocal(nonce, botId, 'failed')
      }
    })()
  }

  retry(entry: Entry) {
    const files = (entry.data.clientNonce && this.outgoingFiles.get(entry.data.clientNonce)) || []
    const text = entry.data.text ?? ''
    if (!text && !files.length) return
    this.entries.set(entry.botId, this.allEntries(entry.botId).filter((e) => e.id !== entry.id))
    this.send(text, entry.botId, entry.threadId ?? null, files, entry.data.clientNonce ?? crypto.randomUUID())
  }

  discard(entry: Entry) {
    if (entry.data.clientNonce) this.outgoingFiles.delete(entry.data.clientNonce)
    this.entries.set(entry.botId, this.allEntries(entry.botId).filter((e) => e.id !== entry.id))
    this.changed()
    this.scheduleSave()
  }

  cancelQueued(_entry: Entry) {
    // Loopback never queues in a relay mailbox.
  }

  private markLocal(nonce: string, botId: string, status: string) {
    const list = this.entries.get(botId)
    const i = list?.findIndex((e) => e.id === `local-${nonce}`) ?? -1
    if (!list || i < 0) return
    const next = [...list]
    next[i] = { ...next[i]!, data: { ...next[i]!.data, status } }
    this.entries.set(botId, next)
    this.changed()
    this.scheduleSave()
  }

  /** A sent file's bytes: cached after the first fetch (sent files never change). */
  async attachmentData(fileId: string, botId: string): Promise<Uint8Array | null> {
    const cached = attachmentCache.get(fileId)
    if (cached) return cached
    const client = this.live
    if (!client) return null
    try {
      const data = await client.readUpload(botId, fileId)
      cacheAttachment(fileId, data)
      return data
    } catch {
      return null
    }
  }

  stop(botId: string) {
    this.perform((c) => c.call('stop', { botId }), true)
  }

  newSession(botId: string) {
    this.perform((c) => c.call('newSession', { botId }))
  }

  logCall(botId: string, seconds: number) {
    this.perform((c) => c.call('logCall', { botId, seconds }))
  }

  screenTakeover(on: boolean) {
    this.perform(async (c) => {
      this.screen = await c.call<ScreenState>('screenTakeover', { on })
      this.changed()
    }, true)
  }

  /** Only the computer itself may turn remote screen on. */
  async setScreenEnabled(on: boolean) {
    this.screen = await (await this.ready()).call<ScreenState>('setScreenEnabled', { enabled: on })
    this.changed()
  }

  /** Product analytics consent: null until the owner chose, undefined before hello or on older hosts. */
  get analytics() {
    return this.hello?.analytics
  }

  /** Only the computer itself may change analytics (the host accepts it from loopback). */
  async setAnalytics(enabled: boolean) {
    const res = await (await this.ready()).call<{ enabled: boolean }>('setAnalytics', { enabled })
    if (this.hello) this.hello = { ...this.hello, analytics: res.enabled }
    this.changed()
  }

  /** Fire-and-forget; the host drops it unless analytics is on. */
  track(event: AnalyticsEvent) {
    void this.withLink((c) => c.call('track', { event })).catch((error: unknown) => console.warn(`track ${event} failed`, error))
  }

  /** Toggles the user's reaction; shown at once, then replaced by the host's copy. */
  react(entry: Entry, emoji: string) {
    const list = this.entries.get(entry.botId)
    const i = list?.findIndex((e) => e.id === entry.id) ?? -1
    if (!list || i < 0) return
    const reactions = [...(list[i]!.data.reactions ?? [])]
    const at = reactions.indexOf(emoji)
    if (at >= 0) reactions.splice(at, 1)
    else reactions.push(emoji)
    const next = [...list]
    next[i] = { ...next[i]!, data: { ...next[i]!.data, reactions } }
    this.entries.set(entry.botId, next)
    this.changed()
    this.perform(async (c) => {
      const res = await c.call<{ entry: Entry }>('react', { entryId: entry.id, emoji })
      this.upsert(res.entry)
      this.changed()
    })
  }

  respond(entry: Entry, option: string | null) {
    if (this.answering.has(entry.id)) return
    this.answering.set(entry.id, option ?? '')
    this.changed()
    void (async () => {
      try {
        await this.withLink((c) => c.call('respondPermission', { entryId: entry.id, optionId: option }), true)
        // The card's own update follows on the events stream; hold the spinner until then.
        await sleep(2000)
      } catch (error) {
        this.lastError = error instanceof Error ? error.message : String(error)
      }
      this.answering.delete(entry.id)
      this.changed()
    })()
  }

  /** The roster's "Mark as read": the chat and all its threads. */
  markAllRead(botId: string) {
    const bot = this.bots.get(botId)
    if (!bot || bot.unread <= 0) return
    this.bots.set(botId, { ...bot, unread: 0 })
    this.changed()
    this.perform((c) => c.call('markRead', { botId, threadId: null, all: true }), true)
  }

  private patchBot(bot: Bot, change: Partial<BotDraft>) {
    const current = this.bots.get(bot.id)
    if (current) this.bots.set(bot.id, { ...current, ...change } as Bot)
    this.changed()
    this.perform((c) => c.call('updateBot', { ...updatePayload(bot), ...change }), true)
  }

  setPinned(bot: Bot, pinned: boolean) {
    this.patchBot(bot, { pinned })
  }

  setHidden(bot: Bot, hidden: boolean) {
    this.patchBot(bot, { hidden })
  }

  /** The roster as the user dragged it, top first: shown at once, then saved on the host. */
  reorder(ids: string[]) {
    ids.forEach((id, position) => {
      const bot = this.bots.get(id)
      if (bot) this.bots.set(id, { ...bot, position })
    })
    this.changed()
    this.perform((c) => c.call('reorderBots', { ids }), true)
  }

  delete(bot: Bot) {
    this.composerDrafts.removeBot(bot.id)
    this.bots.delete(bot.id)
    this.entries.delete(bot.id)
    this.onRosterChanged?.()
    this.changed()
    this.perform((c) => c.call('deleteBot', { botId: bot.id }), true)
  }

  /** Creates a group chat (or opens the one these bots already share) and selects it. */
  async createGroup(name: string, description: string, members: string[]) {
    const draft: GroupDraft = { kind: 'group', name, description, members }
    const res = await (await this.ready()).call<{ bot: Bot }>('createBot', draft)
    this.bots.set(res.bot.id, res.bot)
    this.selection = res.bot.id
    this.onRosterChanged?.()
    this.changed()
    return res.bot
  }

  async updateGroup(draft: GroupDraft) {
    const res = await (await this.ready()).call<{ bot: Bot }>('updateBot', draft)
    this.bots.set(res.bot.id, res.bot)
    this.changed()
  }

  /** Takes one bot out of a group; an automatic "Alice, Bob" name follows the new roster. */
  removeMember(group: Bot, memberId: string) {
    const members = group.members.filter((m) => m !== memberId)
    const autoName = (ids: string[]) => ids.map((id) => this.bots.get(id)?.name).join(', ')
    const name = group.name === autoName(group.members) ? autoName(members) : group.name
    this.bots.set(group.id, { ...group, name, members })
    this.changed()
    const draft: GroupDraft = { id: group.id, kind: 'group', name, description: group.description, members, pinned: group.pinned }
    this.perform((c) => c.call('updateBot', draft), true)
  }

  async loadThread(botId: string, root: string) {
    if (!this.client) return
    try {
      for (const e of await this.client.thread(botId, root)) this.upsert(e)
      this.changed()
    } catch {}
  }

  /** The history between a bot and one peer. Never upserted: old notices would break `loadOlder`'s paging. */
  async botConversation(botId: string, peerId: string): Promise<Entry[]> {
    return (await this.ready()).botConversation(botId, peerId)
  }

  async save(draft: BotDraft) {
    const client = await this.ready()
    const res = draft.id
      ? await client.call<{ bot: Bot }>('updateBot', { ...draft, model: draft.model ?? null })
      : await client.call<{ bot: Bot }>('createBot', draft)
    this.bots.set(res.bot.id, res.bot)
    this.onRosterChanged?.()
    this.changed()
    return res.bot
  }

  async refreshBackends() {
    if (!this.client || !this.hello) return
    try {
      const res = await this.client.call<{ backends: Hello['backends'] }>('refreshBackends', {}, 60_000)
      this.hello = { ...this.hello, backends: res.backends }
      this.changed()
    } catch {}
  }

  async listDirs(path: string | null) {
    return (await this.ready()).call<DirListing>('listDirs', { path })
  }

  async refreshUsage() {
    if (this.retired || !this.client) return
    try {
      this.usage = await this.client.call<Usage>('usage', { refresh: true })
      this.changed()
    } catch {}
  }

  async loadOlder(botId: string) {
    if (!this.client || this.historyComplete.has(botId)) return
    const first = this.chat(botId).find((e) => e.seq > 0 && e.seq !== Number.MAX_SAFE_INTEGER)?.seq ?? Number.MAX_SAFE_INTEGER
    try {
      const older = await this.client.history(botId, first, 100)
      if (older.length < 100) this.historyComplete.add(botId)
      for (const e of older) this.upsert(e)
      this.changed()
    } catch {}
  }

  setSelection(id: string | null) {
    this.selection = id
    this.changed()
  }

  consumeRoutineDraft(botId: string) {
    const value = this.routineDrafts.get(botId)
    if (value !== undefined) {
      this.routineDrafts.delete(botId)
      this.changed()
    }
    return value
  }

  setRoutineDraft(botId: string, text: string) {
    this.routineDrafts.set(botId, text)
    this.changed()
  }
}

/** A full editor update must explicitly clear a previous model override. */
function updatePayload(bot: Bot): BotDraft {
  return {
    id: bot.id, name: bot.name, description: bot.description, avatarColor: bot.avatarColor, avatarShape: bot.avatarShape,
    backend: bot.backend, command: bot.command ?? null, cwd: bot.managedWorkspace ? '' : bot.cwd, permission: bot.permission,
    model: bot.model ?? null, pinned: bot.pinned, hidden: bot.hidden, notify: bot.notify ?? null,
    connectors: bot.connectors, skills: bot.skills, computer: bot.computer,
  }
}

// ponytail: in-memory attachment cache; a disk cache (main process) if relaunches refetching hurts.
const attachmentCache = new Map<string, Uint8Array>()
function cacheAttachment(id: string, data: Uint8Array) {
  attachmentCache.set(id, data)
}

export { isWorking, needsInput }
