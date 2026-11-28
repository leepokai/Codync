import {
  isWorking,
  needsInput,
  normalizeBot,
  unseen,
  type Bot,
  type BotDraft,
  type DirListing,
  type Entry,
  type GroupDraft,
  type Hello,
  type LoginProfile,
  type ReplyQuote,
  type ScreenState,
  type SharedFile,
  type Usage,
} from '@shared/models'
import type { CallLine } from '@shared/calls'
import { localEntryId } from '@shared/threads'
import { pendingCards, type ThreadSetup } from '@shared/work'
import type { HostClient } from '../client/host-client'
import { readSharedFile } from '../../shared/files.ts'
import { errorText } from '../lib/error-text'
import { latestOnly } from '../lib/latest-only'
import { trNow } from '../lib/l10n.ts'
import { prefs } from '../lib/prefs'
import { noteActiveDay } from '../lib/use-kofi-ask'
import { updatePayload } from './bot-draft'
import { FileDownloads } from './file-downloads'
import { BotSections } from './bot-sections'
import { DevServerCommands } from './dev-servers'
import { BotSync, sleep } from './bot-sync'
import { filesToResend, type OutgoingFile } from './outgoing-files.ts'

export { builtInBackendName, type Connection } from './bot-mirror'

/** What a message carries besides its text; `replyTo` shows on the bubble until the computer's own quote replaces it. */
export interface SendOptions {
  files?: OutgoingFile[]
  nonce?: string
  pasted?: string[]
  replyTo?: ReplyQuote | null
  /** The voice call it was said in. */
  call?: string | null
}

/** A thread, as the computer is asked about it: the bot and its task, never the thread's own id. */
export interface ThreadRef {
  botId: string
  taskId: string
}

export const QUICK_REACTIONS = ['👍', '❤️', '😂', '🎉', '👀', '✅']

/** How many older pages opening a task's card loads to find it, like the iPhone. */
const CARD_PAGES = 5

/**
 * One computer's mirror (bots, transcripts), kept by one events stream (catch-up since
 * `rev`, then live). Port of kit's `BotStore`.
 */
export class BotStore extends BotSync {
  readonly fileDownloads = new FileDownloads(() => this.changed())
  readonly sections = new BotSections(() => this.bots, () => this.changed(), (op, undo) => this.perform(op, true, undo))
  readonly devServers = new DevServerCommands((op) => this.perform(op), (op) => this.withLink(op))

  override retire() { this.fileDownloads.retire(); super.retire() }

  private outgoingFiles = new Map<string, OutgoingFile[]>()

  /** The message sent from here last: its row in the queue scrolls into view (`QueuedMessages`). */
  lastSentNonce: string | null = null

  send(text: string, botId: string, { files = [], nonce = crypto.randomUUID(), pasted = [], replyTo = null, call = null }: SendOptions = {}) {
    const trimmed = text.trim()
    if ((!trimmed && !files.length) || this.retired) return
    this.lastSentNonce = nonce
    const data: Entry['data'] = { text: trimmed, status: 'sending', clientNonce: nonce }
    if (call) data.call = call
    if (replyTo) data.replyTo = replyTo
    if (files.length) {
      data.attachments = files.map((f) => ({ id: f.id, name: f.name, size: f.data.length }))
      this.outgoingFiles.set(nonce, files)
    }
    this.upsert({
      id: `local-${nonce}`, seq: Number.MAX_SAFE_INTEGER, botId, rev: 0, kind: 'user',
      turn: 0, data, createdAt: Date.now(), updatedAt: 0,
    })
    this.changed()
    void (async () => {
      try {
        // Safe to repeat: the computer skips a nonce it already has.
        const e = await this.withLink(async (client) => {
          const pending = this.outgoingFiles.get(nonce)
          const ids = pending ? await uploadFiles(client, botId, pending) : null
          return client.send(botId, trimmed, nonce, ids, pasted, replyTo, call)
        }, true)
        this.outgoingFiles.delete(nonce)
        this.upsert(e)
        this.changed()
        // Only once it arrived, so a resend counts once.
        noteActiveDay()
        if (!call) prefs.promptsSent.set(prefs.promptsSent.get() + 1)
      } catch (error) {
        if (this.outgoingFiles.has(nonce)) this.lastError = `Couldn't send the files: ${error instanceof Error ? error.message : error}`
        this.markLocal(nonce, botId, 'failed')
      }
    })()
  }

  /** Asks this computer to update itself while it's behind this app (`installHostUpdate`). */
  requestHostUpdate() {
    this.hostUpdate = { kind: 'requested', at: Date.now() }
    this.changed()
    void this.withLink((c) => c.call('installHostUpdate', {})).catch((error: unknown) => {
      this.hostUpdate = { kind: 'failed', message: error instanceof Error ? error.message : String(error) }
      this.changed()
    })
  }

  retry(entry: Entry) {
    const files = filesToResend(entry, this.outgoingFiles)
    if (!files) return this.setError(trNow("This message's files aren't on this device anymore. Delete it and attach them again."))
    const text = entry.data.text ?? ''
    if (!text && !files.length) return
    this.dropEntry(entry)
    this.send(text, entry.botId, { files, nonce: entry.data.clientNonce, replyTo: entry.data.replyTo })
  }

  discard(entry: Entry) {
    if (entry.data.clientNonce) this.outgoingFiles.delete(entry.data.clientNonce)
    this.dropEntry(entry)
    this.changed()
    this.scheduleSave()
  }

  private dropEntry(entry: Entry) {
    this.entries.set(entry.botId, (this.entries.get(entry.botId) ?? []).filter((e) => e.id !== entry.id))
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
  attachmentData(fileId: string, botId: string): Promise<Uint8Array | null> {
    return this.cachedFile(fileId, (client) => client.readUpload(botId, fileId))
  }

  /** A file a bot shared, read whole and checked: cached after the first fetch (shared files never change). */
  sharedFileData(file: SharedFile, entryId: string): Promise<Uint8Array | null> {
    return this.cachedFile(file.id, (client) => readSharedFile(file, (offset) => client.readFile(entryId, file.id, offset)))
  }

  private async cachedFile(id: string, fetch: (client: HostClient) => Promise<Uint8Array>): Promise<Uint8Array | null> {
    const cached = attachmentCache.get(id)
    if (cached) return cached
    const client = this.live
    if (!client) return null
    try {
      const data = await fetch(client)
      cacheAttachment(id, data)
      return data
    } catch {
      return null
    }
  }

  stop(botId: string) {
    this.perform((c) => c.call('stop', { botId }), true)
  }

  /** Back to the bot's own agent from a fallback: now if it's idle, else from its next turn. The host posts the chat line. */
  switchBack(botId: string) {
    this.perform((c) => c.call('switchBack', { botId }), true)
  }

  /** The bot's agent starts over with a fresh context; the chat stays. */
  newSession(botId: string) {
    this.perform((c) => c.call('newSession', { botId }))
  }

  /** A finished call's line, with the rest of its transcript; the chat then shows the call's messages behind it. */
  async endCall(botId: string, call: string, seconds: number, lines: CallLine[]) {
    try {
      // Safe to repeat: the computer ends a call once.
      const { entry } = await this.withLink((c) => c.call<{ entry: Entry | null }>('endCall', { botId, callId: call, seconds, lines }), true)
      if (entry) this.upsert(entry)
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error)
    }
  }

  /** A call's audio is live: the lead speaks first (only a host with leads marks one, so an older host never gets this). */
  callStarted(botId: string, call: string) {
    if (!this.bots.get(botId)?.lead) return
    // Its opening is the call's, like everything said on it.
    this.perform((c) => c.call('callStarted', { botId, callId: call }))
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

  /** Product analytics on this computer; undefined before hello. */
  get analytics() {
    return this.hello?.analytics
  }

  /** Only the computer itself may change analytics (the host accepts it from loopback). */
  async setAnalytics(enabled: boolean) {
    const res = await (await this.ready()).call<{ enabled: boolean }>('setAnalytics', { enabled })
    if (this.hello) this.hello = { ...this.hello, analytics: res.enabled }
    this.changed()
  }

  /** Whether this computer starts a thread the user asks for themselves; false before hello or on older hosts. */
  get startsThreads() {
    return this.hello?.startThread === true
  }

  /** Threads may start without a bot here (`startThread` without `botId`). */
  get startsThreadsWithoutBot() {
    return this.hello?.threadsWithoutBot === true
  }

  /** A workflow can be built by talking to a thread here (`startThread` with `setup.workflow`). */
  get buildsWorkflowsInThreads() {
    return this.hello?.workflowThreads === true
  }

  /** A thread can be started on a login of its agent here (`startThread` with `setup.loginProfile`). */
  get startsThreadsOnLogins() {
    return this.hello?.threadLogins === true
  }

  /** A thread can start with files here (`startThread` with `attachments`). */
  get startsThreadsWithFiles() {
    return this.hello?.threadFiles === true
  }

  /** Bots have a list of projects and a worktree mode here; their `cwd` is always their own folder. */
  get hasProjects() {
    return this.hello?.projects === true
  }

  /** A new thread can name its place in a git project here (`startThread` with `setup.place`). */
  get startsThreadsInPlace() {
    return this.hello?.threadPlace === true
  }

  /** Bots have shared folders here, linked into each new worktree of a project. */
  get fillsWorktrees() {
    return this.hello?.worktreeFill === true
  }

  /** A thread without a bot sent no folder gets one of its own here (an older host: the home folder). */
  get givesThreadsOwnFolders() {
    return this.hello?.threadFolders === true
  }

  /** A routine can name the folder its runs work in here (`saveRoutine` with `folder`). */
  get routinesHaveFolders() {
    return this.hello?.routineFolder === true
  }

  /** A thread the user starts with `botId` themselves (null: without a bot, on the agent `setup` names), opened by
   *  `text`, its task on `setup` where that differs from the bot's: where its task is, the bot's (or the box's) id
   *  and the task's. */
  async startThread(botId: string | null, text: string, setup: ThreadSetup = {}, files: OutgoingFile[] = []): Promise<{ botId: string; taskId: string }> {
    // Its files go to the bot first (without one, to the computer's box of threads without a bot).
    const started = await this.withLink(async (c) => {
      const attachments = files.length ? await uploadFiles(c, botId ?? '', files) : undefined
      return c.call<{ botId: string; taskId: string }>('startThread', { ...(botId ? { botId } : {}), text: text.trim(), setup, ...(attachments ? { attachments } : {}) })
    })
    noteActiveDay()
    return started
  }

  /** Whether a bot with a new message moves to the top of this computer's roster; undefined before hello or on older hosts. */
  get autoOrder() {
    return this.hello?.autoOrder
  }

  /** Shown at once, put back if the computer refuses. */
  async setAutoOrder(enabled: boolean) {
    this.showAutoOrder(enabled)
    try {
      await (await this.ready()).call('setAutoOrder', { enabled })
    } catch (e) {
      this.showAutoOrder(!enabled)
      throw e
    }
  }

  /** Days after a thread last ended that this computer deletes it (0: never); undefined before hello or on older hosts. */
  get threadKeepDays() {
    return this.hello?.threadKeepDays
  }

  /** Shown at once, put back if the computer refuses. */
  async setThreadKeepDays(days: number) {
    const before = this.hello?.threadKeepDays
    this.showThreadKeepDays(days)
    try {
      const res = await (await this.ready()).call<{ days: number }>('setThreadKeepDays', { days })
      this.showThreadKeepDays(res.days)
    } catch (e) {
      if (before !== undefined) this.showThreadKeepDays(before)
      throw e
    }
  }

  /** The Threads list's first page of older threads on this computer, once. */
  loadFirstOlderThreads() {
    if (!this.olderThreads.started) this.loadOlderThreads()
  }

  /** The Threads list's next page of older threads, as the list reaches its end. Only while connected to a host that
   * lists them. */
  loadOlderThreads() {
    if (this.hello?.threadKeepDays === undefined || this.connection.kind !== 'online') return
    this.olderThreads
      .loadNext(async (before) => (await this.ready()).call('olderThreads', before === undefined ? {} : { before }))
      .catch((e: unknown) => this.setError(errorText(e)))
  }

  /** The user's name for this computer, empty for its hostname (a cloud computer is named in Codync Cloud). */
  async setComputerName(name: string) {
    try {
      const res = await (await this.ready()).call<{ name: string }>('setComputerName', { name })
      this.setHostName(res.name)
    } catch (e) {
      this.setError(errorText(e))
    }
  }

  /** Deletes the credentials saved on this computer, for one whose key is gone; agents' own sign-ins stay. */
  async resetCredentials() {
    try {
      const res = await (await this.ready()).call<{ backends: Hello['backends'] }>('resetCredentials', {}, 60_000)
      this.showBackends(res.backends)
    } catch (e) {
      this.setError(errorText(e))
    }
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

  /** A card's button: `entry` shows `choice` as answering (its spinner) while `op` runs. */
  private answer(entry: Entry, choice: string, op: (c: HostClient) => Promise<unknown>) {
    if (this.answering.has(entry.id)) return
    this.answering.set(entry.id, choice)
    this.changed()
    void (async () => {
      try {
        await this.withLink(op, true)
      } catch (error) {
        this.lastError = error instanceof Error ? error.message : String(error)
      }
      this.answering.delete(entry.id)
      this.changed()
    })()
  }

  respond(entry: Entry, option: string | null) {
    this.answer(entry, option ?? '', async (c) => {
      await c.call('respondPermission', { entryId: entry.id, optionId: option })
      // The card's own update follows on the events stream; hold the spinner until then.
      await sleep(2000)
    })
  }

  /** Presses one of a choice card's options (`request_decision`): the choice is the bot's answer and the user's
   *  next message, both from the computer. */
  decide(entry: Entry, option: string) {
    this.answer(entry, option, async (c) => {
      await c.call('respondDecision', { entryId: entry.id, option })
      // The card's own update follows on the events stream; hold the spinner until then.
      await sleep(2000)
    })
  }

  /** Has a queued message handled now (`steer`): the computer's update takes it out of the queue. One in a
   *  `thread` is named by its bot and task, never by its own id. */
  steer(entry: Entry, thread?: ThreadRef) {
    this.answer(entry, 'steer', (c) => c.call('steer', { botId: thread?.botId ?? entry.botId, taskId: thread?.taskId, entryId: entry.id }))
  }

  /** Takes back a queued message (`unqueue`): gone at once, back if the computer refuses. */
  unqueue(entry: Entry, thread?: ThreadRef) {
    const show = (e: Entry) => {
      if (thread) this.mergeThread(entry.botId, [e])
      else this.upsert(e)
      this.changed()
    }
    show({ ...entry, data: { ...entry.data, status: 'cancelled', removed: true } })
    this.perform((c) => c.call('unqueue', { botId: thread?.botId ?? entry.botId, taskId: thread?.taskId, entryId: entry.id }), true, () => show(entry))
  }

  /**
   * Writes to the thread of the bot's task (`send` with `taskId`): the message shows in the thread's transcript at
   * once, and the computer's own entry replaces it (queued while the thread's turn runs; the first one to an ended
   * task reopens it). A refusal leaves it as failed and says why. `nonce` again sends the same message once more.
   */
  sendToThread(thread: ThreadRef, threadId: string, text: string, { files = [], nonce = crypto.randomUUID() }: { files?: OutgoingFile[]; nonce?: string } = {}) {
    const trimmed = text.trim()
    if ((!trimmed && !files.length) || this.retired) return
    const show = (e: Entry) => {
      this.mergeThread(threadId, [e])
      this.changed()
    }
    const data: Entry['data'] = { text: trimmed, status: 'sending', clientNonce: nonce }
    if (files.length) {
      data.attachments = files.map((f) => ({ id: f.id, name: f.name, size: f.data.length }))
      this.outgoingFiles.set(nonce, files)
    }
    const local: Entry = {
      id: localEntryId(nonce), seq: Number.MAX_SAFE_INTEGER, botId: threadId, rev: 0, kind: 'user',
      turn: 0, data, createdAt: Date.now(), updatedAt: 0,
    }
    show(local)
    // Safe to repeat: the computer answers a nonce it already has with the same entry.
    void this.withLink(async (c) => {
      const pending = this.outgoingFiles.get(nonce)
      const attachments = pending ? await uploadFiles(c, thread.botId, pending, thread.taskId) : undefined
      return c.call<{ entry: Entry }>('send', { ...thread, text: trimmed, clientNonce: nonce, ...(attachments ? { attachments } : {}) })
    }, true).then(
      (res) => {
        noteActiveDay()
        this.outgoingFiles.delete(nonce)
        show({ ...res.entry, data: res.entry.data ?? {} })
      },
      (error: unknown) => {
        this.lastError = errorText(error)
        // Unless the computer's copy came in on the events stream after all.
        if (this.threadEntriesOf(threadId).some((e) => e.id === local.id)) show({ ...local, data: { ...local.data, status: 'failed' } })
        else this.changed()
      },
    )
  }

  /** Resend on a message that never reached the thread: the same nonce, with its files while this app still holds them. */
  resendToThread(thread: ThreadRef, entry: Entry) {
    const files = filesToResend(entry, this.outgoingFiles)
    if (!files) return this.setError(trNow("This message's files aren't on this device anymore. Delete it and attach them again."))
    this.sendToThread(thread, entry.botId, entry.data.text ?? '', { files, nonce: entry.data.clientNonce })
  }

  /** Takes a message that never reached the thread out of its transcript. */
  discardFromThread(entry: Entry) {
    if (entry.data.clientNonce) this.outgoingFiles.delete(entry.data.clientNonce)
    this.mergeThread(entry.botId, [{ ...entry, data: { ...entry.data, removed: true } }])
    this.changed()
  }

  /** Saves the user's version of a handoff that hasn't been sent; whether it did. */
  async editHandoff(entry: Entry, text: string) {
    try {
      await this.withLink((c) => c.call('editHandoff', { entryId: entry.id, text }), true)
      return true
    } catch (error) {
      this.lastError = error instanceof Error ? error.message : String(error)
      this.changed()
      return false
    }
  }

  /** Carries on with the bot's current conversation instead of the handoff. */
  keepConversation(entry: Entry) {
    this.answer(entry, 'handoff', (c) => c.call('keepConversation', { entryId: entry.id }))
  }

  /** Runs a failed routine run's work again, from its notice; the notice's update drops the button. */
  retryRoutineRun(entry: Entry) {
    const runId = entry.data.runId
    if (runId) this.answer(entry, 'retry', (c) => c.call('retryRoutineRun', { botId: entry.botId, runId }))
  }

  /** Continue on a chain stopped at its handoff limit: the bot gets "Continue" from the user; the notice's update drops the button. */
  continueChain(entry: Entry) {
    this.answer(entry, 'continue', (c) => c.call('continueChain', { entryId: entry.id }))
  }

  /** The roster's "Mark as read": cards still waiting on the user stay counted until they're answered. */
  markAllRead(botId: string) {
    const bot = this.bots.get(botId)
    if (!bot || unseen(bot) <= 0) return
    this.bots.set(botId, { ...bot, unread: bot.waiting })
    this.changed()
    this.perform((c) => c.call('markRead', { botId }), true)
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

  /** Whether the bot may see and use the computer's screen. */
  setComputer(bot: Bot, computer: boolean) {
    this.patchBot(bot, { computer })
  }

  /** Whether the bot uses the memory all bots share. */
  setSharedMemory(bot: Bot, sharedMemory: boolean) {
    this.patchBot(bot, { sharedMemory })
  }

  /** Whether the bot's harness keeps its own memory on beside Codync's. */
  setHarnessMemory(bot: Bot, harnessMemory: boolean) {
    this.patchBot(bot, { harnessMemory })
  }

  /** Makes `bot` the lead, in place of the one before; a refusal gives the star back. */
  setLead(bot: Bot) {
    const before = [...this.bots.values()].find((b) => b.lead)?.id
    const show = (lead: string | undefined) => {
      for (const [id, b] of this.bots) this.bots.set(id, { ...b, lead: id === lead })
      this.changed()
    }
    show(bot.id)
    this.perform((c) => c.call('setLead', { botId: bot.id }), true, () => show(before))
  }

  setHidden(bot: Bot, hidden: boolean) {
    this.patchBot(bot, { hidden })
  }

  /** Paused: the bot answers nothing and spends nothing until it is resumed. */
  setPaused(bot: Bot, paused: boolean) {
    this.patchBot(bot, { paused })
  }

  setFastMode(bot: Bot, fastMode: boolean) {
    this.patchBot(bot, { fastMode })
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
    const bot = this.putHostBot(normalizeBot(res.bot))
    this.selection = bot.id
    this.onRosterChanged?.()
    this.changed()
    return bot
  }

  async updateGroup(draft: GroupDraft) {
    const res = await (await this.ready()).call<{ bot: Bot }>('updateBot', draft)
    this.putHostBot(normalizeBot(res.bot))
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

  /** What a voice call carried, from the computer: the loaded chat may not reach back to it. */
  async callEntries(botId: string, call: string): Promise<Entry[]> {
    return (await (await this.ready()).call<{ entries: Entry[] }>('callEntries', { botId, callId: call })).entries
  }

  /** A thread's transcript, from the computer; it also goes into the watched thread's (`threadEntriesOf`). */
  async loadThread(botId: string, threadId: string): Promise<Entry[]> {
    const res = await (await this.ready()).call<{ entries: Entry[] }>('threadEntries', { botId, threadId })
    const entries = res.entries.map((e) => ({ ...e, data: e.data ?? {} }))
    this.mergeThread(threadId, entries)
    this.changed()
    return entries
  }

  /** Stops one working task of the bot's box, or all of them without `taskId`. */
  stopWork(botId: string, taskId?: string) {
    this.perform((c) => c.call('stopWork', taskId ? { botId, taskId } : { botId }))
  }

  /** Moves a waiting task to the front of the box. */
  workFirst(botId: string, taskId: string) {
    this.perform((c) => c.call('workFirst', { botId, taskId }))
  }

  /** Starts a waiting task now: the working task `insteadOf` gives way and starts again later. */
  workCutIn(botId: string, taskId: string, insteadOf: string) {
    this.perform((c) => c.call('workCutIn', { botId, taskId, insteadOf }))
  }

  /** Takes a waiting task, or one that ended, out of the box. */
  dropWork(botId: string, taskId: string) {
    // An older thread leaves the list at once; it comes back if the computer refuses.
    this.perform((c) => c.call('dropWork', { botId, taskId }), false, this.olderThreads.drop(botId, taskId))
  }

  /** Puts a stopped or failed task back in the box, last. */
  retryWork(botId: string, taskId: string) {
    this.perform((c) => c.call('retryWork', { botId, taskId }))
  }

  /** The history between a bot and one peer. Never upserted: old notices would break `loadOlder`'s paging. */
  async botConversation(botId: string, peerId: string): Promise<Entry[]> {
    return (await this.ready()).botConversation(botId, peerId)
  }

  /** What `backend` already has in the user's home, not the Codync skills and connectors a bot passes on. */
  async agentGear(backend: string): Promise<{ skills: AgentGearItem[]; connectors: AgentGearItem[]; rules: AgentGearItem[]; plugins: AgentGearItem[] }> {
    const client = this.client
    if (!client) throw new Error("Couldn't load what this agent brings.")
    const value = await client.call<{ skills?: AgentGearItem[]; connectors?: AgentGearItem[]; rules?: AgentGearItem[]; plugins?: AgentGearItem[] }>('agentGear', { backend })
    return { skills: value.skills ?? [], connectors: value.connectors ?? [], rules: value.rules ?? [], plugins: value.plugins ?? [] }
  }

  async save(draft: BotDraft) {
    const client = await this.ready()
    const res = draft.id
      ? await client.call<{ bot: Bot }>('updateBot', { ...draft, model: draft.model ?? null })
      : await client.call<{ bot: Bot }>('createBot', draft)
    const bot = this.putHostBot(normalizeBot(res.bot))
    this.onRosterChanged?.()
    this.changed()
    return bot
  }

  /** Shows the agent list a call answered with. */
  private showBackends(backends: Hello['backends']) {
    if (!this.hello) return
    this.hello = { ...this.hello, backends }
    this.changed()
  }

  async refreshBackends() {
    if (!this.client || !this.hello) return
    try {
      this.showBackends((await this.client.call<{ backends: Hello['backends'] }>('refreshBackends', {}, 60_000)).backends)
    } catch {}
  }

  /** A new login profile for agent `backend`. An `account` one is signed out (a `login` setup with it signs it in); an `apiKey` one has no key yet. */
  async addLoginProfile(backend: string, kind: LoginProfile['kind']) {
    const added = await (await this.ready()).call<{ loginProfile: LoginProfile; backends: Hello['backends'] }>('addLoginProfile', { backend, kind })
    this.showBackends(added.backends)
    return added.loginProfile.id
  }

  /** Asks the login profile's agent whether it's signed in, and as whom. */
  async checkLoginProfile(backend: string, loginProfile: string) {
    this.showBackends((await (await this.ready()).call<{ backends: Hello['backends'] }>('agentAuth', { backend, loginProfile }, 60_000)).backends)
  }

  /** Has the login profile's agent sign it in itself, in a browser on the computer; answers once that's done. */
  async signInLoginProfile(backend: string, loginProfile: string) {
    this.showBackends((await (await this.ready()).call<{ backends: Hello['backends'] }>('agentAuthenticate', { backend, loginProfile }, 660_000)).backends)
  }

  /** Names a login profile, or with `null` the default one; an empty name clears it. */
  async renameLoginProfile(backend: string, loginProfile: string | null, name: string) {
    this.showBackends((await (await this.ready()).call<{ backends: Hello['backends'] }>('renameLoginProfile', { backend, loginProfile: loginProfile ?? undefined, name })).backends)
  }

  async removeLoginProfile(loginProfile: string) {
    this.showBackends((await (await this.ready()).call<{ backends: Hello['backends'] }>('removeLoginProfile', { loginProfile }, 60_000)).backends)
  }

  /** An API key profile's variables: `vars` as typed (an empty value removes one), and `imported` names whose values the host reads from the login shell now. */
  async setLoginProfileKeys(loginProfile: string, vars: Record<string, string>, imported: string[] = []) {
    this.showBackends((await (await this.ready()).call<{ backends: Hello['backends'] }>('setLoginProfileKeys', { loginProfile, vars, import: imported }, 60_000)).backends)
  }

  /** The names (never the values) of the login shell's variables, which a login profile can import. */
  async loginShellVariables() {
    return (await (await this.ready()).call<{ names: string[] }>('loginShellVariables', {}, 60_000)).names
  }

  async selectWorkspace(path: string) {
    return (await this.ready()).call<{ path: string }>('selectWorkspace', { path })
  }

  async listDirs(path: string | null) {
    return (await this.ready()).call<DirListing>('listDirs', { path })
  }

  /** Whether this computer lists one agent's limits. */
  async setUsageShown(id: string, shown: boolean) {
    this.usage = await (await this.ready()).call<Usage>('setUsageShown', { id, shown })
    this.changed()
  }

  /** The order this computer lists its agents in. */
  setUsageOrder = latestOnly(async (ids: string[]) => {
    this.usage = await (await this.ready()).call<Usage>('setUsageOrder', { ids })
    this.changed()
  })

  async refreshUsage() {
    if (this.retired || !this.client) return
    try {
      this.usage = await this.client.call<Usage>('usage', { refresh: true })
      this.changed()
    } catch {}
  }

  async loadOlder(botId: string) {
    // Held while everything is fetched again, like `canLoadOlder`.
    if (!this.client || this.refetched || this.historyComplete.has(botId)) return
    const first = this.allEntries(botId).find((e) => e.seq > 0 && e.seq !== Number.MAX_SAFE_INTEGER)?.seq ?? Number.MAX_SAFE_INTEGER
    try {
      const older = await this.client.history(botId, first, 100)
      if (older.length < 100) this.historyComplete.add(botId)
      for (const e of older) this.upsert(e)
      this.changed()
    } catch {}
  }

  /** The card waiting in the bot's chat for task `taskId` (`pendingCards`: the one its row reads), older pages loaded
   * until it is there (`CARD_PAGES` at most). */
  async cardOf(botId: string, taskId: string): Promise<Entry | undefined> {
    let found = pendingCards(this.allEntries(botId)).get(taskId)
    for (let page = 0; !found && page < CARD_PAGES && this.canLoadOlder(botId); page++) {
      await this.loadOlder(botId)
      found = pendingCards(this.allEntries(botId)).get(taskId)
    }
    return found
  }

  setSelection(id: string | null) {
    this.selection = id
    this.changed()
  }
}

// ponytail: in-memory attachment cache; a disk cache (main process) if relaunches refetching hurts.
const attachmentCache = new Map<string, Uint8Array>()
/** Uploads `files` for one message (to the thread of `taskId` when given), each cached as sent; their upload ids. */
async function uploadFiles(client: HostClient, botId: string, files: OutgoingFile[], taskId?: string): Promise<string[]> {
  const ids: string[] = []
  for (const file of files) {
    const id = crypto.randomUUID()
    await client.upload(botId, id, file.name, file.data, taskId)
    cacheAttachment(id, file.data)
    ids.push(id)
  }
  return ids
}

function cacheAttachment(id: string, data: Uint8Array) {
  attachmentCache.set(id, data)
}

export { isWorking, needsInput }

/** One skill or connector an agent keeps for itself (`agentGear`). */
export interface AgentGearItem {
  name: string
  description?: string
  detail?: string
}
