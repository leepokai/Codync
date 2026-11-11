import {
  isWorking,
  isInternalEntry,
  needsInput,
  type AgentAccount,
  type CreatedAppLink,
  type AppLinkMode,
  type BotApp,
  normalizeBot,
  type Bot,
  type BotDraft,
  type DirListing,
  type Entry,
  type GroupDraft,
  type Hello,
  type MessageBookmark,
  type OpenItem,
  type ReplyQuote,
  type ScreenState,
  type SentinelReport,
  type SharedFile,
  type ThreadInfo,
  type Usage,
  type WeekSummary,
} from '@shared/models'
import type { BotCatalog } from '@shared/catalog'
import type { CallLine } from '@shared/calls'
import { cleanRules, type ApprovalEvaluation, type ApprovalRule, type RuleProposal } from '@shared/approval-rules'
import type { HostHealth } from '@shared/health'
import type { SecurityProfile } from '@shared/security-profiles'
import type { HostClient } from '../client/host-client'
import { readSharedFile } from '../../shared/files.ts'
import { FileDownloads } from './file-downloads'
import { BotSections } from './bot-sections'
import { BotSync, sleep } from './bot-sync'
import { DocumentsStore } from './documents-store'
import type { VoiceTranscript } from '../lib/voice-transcript'
import { createAppShareLink } from '../lib/app-links'

export { builtInBackendName, type Connection } from './bot-mirror'

/** A file picked in the composer, sent with the next message. */
export interface OutgoingFile {
  id: string
  name: string
  data: Uint8Array
}

/** What a message carries besides its text; `replyTo` shows on the bubble until the computer's own quote replaces it. */
export interface SendOptions {
  files?: OutgoingFile[]
  nonce?: string
  pasted?: string[]
  replyTo?: ReplyQuote | null
  /** A document's side conversation (`doc:<id>`) instead of the chat. */
  thread?: string | null
  /** The voice call it was said in. */
  call?: string | null
}

/** Anything larger is refused by the computer. */
export const MAX_FILE_SIZE = 100 * 1024 * 1024

export const QUICK_REACTIONS = ['Ã°Å¸â€˜Â', 'Ã¢ÂÂ¤Ã¯Â¸Â', 'Ã°Å¸Ëœâ€š', 'Ã°Å¸Å½â€°', 'Ã°Å¸â€˜â‚¬', 'Ã¢Å“â€¦']

/**
 * One computer's mirror (bots, transcripts), kept by one events stream (catch-up since
 * `rev`, then live). Port of kit's `BotStore`.
 */
export class BotStore extends BotSync {
  private documentStores = new Map<string, DocumentsStore>()
  routineDrafts = new Map<string, string>()
  documentsFor(botId: string) {
    let documents = this.documentStores.get(botId)
    if (!documents) {
      documents = new DocumentsStore(botId, () => this.ready(), JSON.stringify([this.contextId, this.computer.id]))
      this.documentStores.set(botId, documents)
    }
    return documents
  }
  readonly fileDownloads = new FileDownloads(() => this.changed())
  readonly sections = new BotSections(() => this.bots, () => this.changed(), (op, undo) => this.perform(op, true, undo))

  override retire() {
    for (const documents of this.documentStores.values()) documents.retire()
    this.documentStores.clear(); this.fileDownloads.retire(); super.retire()
  }

  private outgoingFiles = new Map<string, OutgoingFile[]>()

  /** How much of a file the computer has, by the file's own id (shown in the composer while it uploads). */
  readonly uploadPercents = new Map<string, number>()

  /** The message sent from here last: its row in the queue scrolls into view (`QueuedMessages`). */
  lastSentNonce: string | null = null

  send(text: string, botId: string, { files = [], nonce = crypto.randomUUID(), pasted = [], replyTo = null, thread = null, call = null }: SendOptions = {}) {
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
      turn: 0, data, createdAt: Date.now(), updatedAt: 0, ...(thread ? { threadId: thread } : {}),
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
              await client.upload(botId, id, file.name, file.data, (sent) => {
                this.uploadPercents.set(file.id, Math.min(99, Math.floor((sent * 100) / Math.max(1, file.data.length))))
                this.changed()
              })
              cacheAttachment(id, file.data)
              this.uploadPercents.delete(file.id)
              this.changed()
              ids.push(id)
            }
          }
          return client.send(botId, trimmed, nonce, ids, pasted, replyTo?.id ?? null, thread, call)
        }, true)
        this.outgoingFiles.delete(nonce)
        this.upsert(e)
        this.changed()
      } catch (error) {
        if (this.outgoingFiles.has(nonce)) this.lastError = `Couldn't send the files: ${error instanceof Error ? error.message : error}`
        for (const file of this.outgoingFiles.get(nonce) ?? []) this.uploadPercents.delete(file.id)
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
    const files = (entry.data.clientNonce && this.outgoingFiles.get(entry.data.clientNonce)) || []
    const text = entry.data.text ?? ''
    if (!text && !files.length) return
    this.entries.set(entry.botId, (this.entries.get(entry.botId) ?? []).filter((e) => e.id !== entry.id))
    this.dropEntry(entry)
    this.send(text, entry.botId, { files, nonce: entry.data.clientNonce, replyTo: entry.data.replyTo })
  }

  discard(entry: Entry) {
    if (entry.data.clientNonce) this.outgoingFiles.delete(entry.data.clientNonce)
    this.entries.set(entry.botId, (this.entries.get(entry.botId) ?? []).filter((e) => e.id !== entry.id))
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

  /** Interrupts every bot on this computer that is working or waiting on the user. */
  stopAll() {
    this.perform((c) => c.call('stopAll', {}), true)
  }

  openMessageContext(entries: Entry[], hit: { botId: string; id: string; threadId: string | null }) {
    this.messageContext = { ...hit, entries: entries.filter((e) => !isInternalEntry(e)) }
    this.changed()
  }

  closeMessageContext() {
    this.messageContext = null
    this.changed()
  }

  newSession(botId: string) {
    this.perform((c) => c.call('newSession', { botId }))
  }

  /** Back to the bot's own agent from a fallback: now if it's idle, else from its next turn. The host posts the chat line. */
  switchBack(botId: string) {
    this.perform((c) => c.call('switchBack', { botId }), true)
  }

  logCall(botId: string, seconds: number, callId: string = crypto.randomUUID(), transcript?: VoiceTranscript) {
    this.perform((c) => c.call('logCall', { botId, seconds, callId, ...(transcript ? { transcript } : {}) }))
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

  /** The security profiles the host offers; null from an older host. */
  async securityProfiles(): Promise<SecurityProfile[] | null> {
    try {
      return (await (await this.ready()).call<{ profiles: SecurityProfile[] }>('securityProfiles', {})).profiles
    } catch {
      return null
    }
  }

  /** The rule "always allow" on this card would keep; null when the host can't say (an older one). */
  async proposeRule(entry: Entry): Promise<RuleProposal | null> {
    try {
      return await (await this.ready()).call<RuleProposal>('proposeRule', { entryId: entry.id })
    } catch {
      return null
    }
  }

  /** Answers a card; with `rule`, an "always allow" also keeps that rule for the bot. */
  respond(entry: Entry, option: string | null, rule?: ApprovalRule) {
    this.answer(entry, option ?? '', async (c) => {
      await c.call('respondPermission', { entryId: entry.id, optionId: option, ...(rule ? { rule } : {}) })
      // The card's own update follows on the events stream; hold the spinner until then.
      await sleep(2000)
    })
  }

  /** What the bot would do with `command` under `rules` (the saved ones when omitted); null when the host can't say. */
  async testApprovalRule(
    botId: string,
    command: string,
    rules?: ApprovalRule[],
    quick?: { on: boolean; policies: Record<string, string> },
  ): Promise<ApprovalEvaluation | null> {
    try {
      const body = {
        botId,
        command,
        ...(rules ? { rules: cleanRules(rules) } : {}),
        ...(quick ? { actionRulesOn: quick.on, actionRules: quick.policies } : {}),
      }
      return (await (await this.ready()).call<{ evaluation: ApprovalEvaluation }>('testApprovalRule', body)).evaluation
    } catch {
      return null
    }
  }

  /** Continues a turn that stopped half done, from its card; the host refuses a card already used. */
  async continueTurn(entry: Entry): Promise<string | null> {
    try {
      await (await this.ready()).call('continueTurn', { entryId: entry.id })
      return null
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  }

  /** Moves a waiting message into the running turn instead of after it; why not, when refused. */
  async steer(entry: Entry): Promise<string | null> {
    try {
      await (await this.ready()).call('steer', { botId: entry.botId, entryId: entry.id })
      return null
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  }

  /** Rewrites a waiting message; the agent reads it when its turn comes. */
  async editQueued(entry: Entry, text: string): Promise<string | null> {
    try {
      await (await this.ready()).call('editQueued', { botId: entry.botId, entryId: entry.id, text })
      return null
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  }

  /** Drops a waiting message: the agent never hears it. */
  async deleteQueued(entry: Entry): Promise<string | null> {
    try {
      await (await this.ready()).call('deleteQueued', { botId: entry.botId, entryId: entry.id })
      return null
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  }

  /** Has a queued message handled now (`steer`): the computer's update takes it out of the queue. */
  steer(entry: Entry) {
    this.answer(entry, 'steer', (c) => c.call('steer', { botId: entry.botId, entryId: entry.id }))
  }

  /** Takes back a queued message (`unqueue`): gone at once, back if the computer refuses. */
  unqueue(entry: Entry) {
    const show = (e: Entry) => {
      this.upsert(e)
      this.changed()
    }
    show({ ...entry, data: { ...entry.data, status: 'cancelled', removed: true } })
    this.perform((c) => c.call('unqueue', { botId: entry.botId, entryId: entry.id }), true, () => show(entry))
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

  undoBeliefObservation(entry: Entry) {
    const learning = entry.data.beliefLearning
    if (learning?.status === 'active') this.answer(entry, 'undo-learning', (c) => c.call('undoBeliefObservation', {
      botId: entry.botId, entryId: entry.id, revision: learning.revision, shared: learning.shared,
    }))
  }

  changeBelief(entry: Entry, method: 'confirmBelief' | 'forgetBelief' | 'restoreBelief') {
    this.answer(entry, method, (c) => c.call(method, { botId: entry.botId, entryId: entry.id }))
  }

  /** Runs a failed routine run's work again, from its notice; the notice's update drops the button. */
  retryRoutineRun(entry: Entry) {
    const runId = entry.data.runId
    if (runId) this.answer(entry, 'retry', (c) => c.call('retryRoutineRun', { botId: entry.botId, runId }))
  }

  /** Ends a stopped turn from its card; the messages stay and the card can't be continued. */
  async cancelTask(entry: Entry): Promise<string | null> {
    try {
      await (await this.ready()).call('cancelTask', { entryId: entry.id })
      return null
    } catch (error) {
      return error instanceof Error ? error.message : String(error)
    }
  }

  /** Ends a failed routine run from its notice; the messages stay and no retry is left. */
  cancelRoutineRun(entry: Entry) {
    this.answer(entry, 'cancel', (c) => c.call('cancelTask', { entryId: entry.id }))
  }

  /** The roster's "Mark as read". */
  markAllRead(botId: string) {
    const bot = this.bots.get(botId)
    if (!bot || bot.unread <= 0) return
    this.bots.set(botId, { ...bot, unread: 0 })
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

  /** What the bot waits on: open approvals, routine runs and unread threads (derived by the host). */
  async botTasks(botId: string): Promise<{
    approvals: { id: string; title: string; detail: string; since: number }[]
    runs: { id: string; routineId: string; status: string; startedAt: number | null; finishedAt: number | null; detail: string | null; threadRoot: string | null }[]
    threads: { id: string; unread: number }[]
  }> {
    return (await this.ready()).call('botTasks', { botId })
  }

  /** Copies the bot's setup into a new one; the host names it and adds it to the roster. */
  async duplicateBot(bot: Bot): Promise<Bot> {
    const res = await (await this.ready()).call<{ bot: Bot }>('duplicateBot', { botId: bot.id })
    const copy = this.putHostBot(normalizeBot(res.bot))
    this.onRosterChanged?.()
    this.changed()
    return copy
  }

  /** Creates a group chat (or opens the one these bots already share) and selects it. */
  async createGroup(name: string, description: string, members: string[], chiefBotId: string | null = null) {
    const draft: GroupDraft = { kind: 'group', name, description, members, chiefBotId }
    // (handled above)
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

  /** The host's ready-made bots and coordinator teams. */
  async botCatalog(): Promise<BotCatalog> {
    return (await this.ready()).call<BotCatalog>('botCatalog', {})
  }

  /** Creates a catalog team on the host (its coordinator, its teammates and their group) and opens the group. */
  async installTeam(id: string, roleModels?: Record<string, string>) {
    const client = await this.ready()
    if (roleModels && Object.keys(roleModels).length) {
      const catalog = await client.call<BotCatalog>('botCatalog', {})
      if (!catalog.capabilities?.roleModels) throw new Error('This computer does not support models by role. Reconnect or remove those choices.')
    }
    const custom = !!roleModels && Object.keys(roleModels).length > 0
    const res = await client.call<{ bot: Bot }>(custom ? 'installTeamWithRoleModels' : 'installTeam', { id, ...(custom && { roleModels }) }, 60_000)
    const group = normalizeBot(res.bot)
    this.bots.set(group.id, group)
    this.selection = group.id
    this.onRosterChanged?.()
    this.changed()
    return group
  }

  /** Creates a marketplace agent's bot (its look, instructions, skills and routines) and opens it. */
  async installAgent(id: string) {
    const res = await (await this.ready()).call<{ bot: Bot }>('installAgent', { id }, 120_000)
    const bot = normalizeBot(res.bot)
    this.bots.set(bot.id, bot)
    this.selection = bot.id
    this.onRosterChanged?.()
    this.changed()
    return bot
  }

  /** Takes one bot out of a group; an automatic "Alice, Bob" name follows the new roster. */
  removeMember(group: Bot, memberId: string) {
    const members = group.members.filter((m) => m !== memberId)
    const autoName = (ids: string[]) => ids.map((id) => this.bots.get(id)?.name).join(', ')
    const name = group.name === autoName(group.members) ? autoName(members) : group.name
    this.bots.set(group.id, { ...group, name, members })
    this.changed()
    const draft: GroupDraft = { id: group.id, kind: 'group', name, description: group.description, members, pinned: group.pinned, chiefBotId: group.chiefBotId ?? null }
    this.perform((c) => c.call('updateBot', draft), true)
  }

  /** A chat's threads, by bot: the live drawer and the archive (never the lines). */
  threads = new Map<string, ThreadInfo[]>()
  archivedThreads = new Map<string, ThreadInfo[]>()

  /** One thread's lines, with its row for the title and branch line. */
  async loadThread(botId: string, root: string) {
    const { entries, thread } = await (await this.ready()).thread(botId, root)
    if (this.retired) return null
    for (const entry of entries) this.upsert(entry)
    this.changed()
    return thread
  }

  async refreshThreads(botId: string, archived = false) {
    const list = await (await this.ready()).threads(botId, archived)
    if (this.retired) return
    ;(archived ? this.archivedThreads : this.threads).set(botId, list)
    this.changed()
  }

  /** Opens a thread, optionally branched from one of the chat's messages, and answers its root. */
  async startThread(botId: string, fromEntryId: string | null = null, title = '') {
    const id = await (await this.ready()).createThread(botId, title, fromEntryId)
    await this.refreshThreads(botId)
    return id
  }

  async renameThread(botId: string, threadId: string, title: string) {
    const named = title.trim()
    await (await this.ready()).renameThread(threadId, named)
    const rename = (list: ThreadInfo[]) => list.map((t) => (t.id === threadId ? { ...t, title: named } : t))
    for (const map of [this.threads, this.archivedThreads]) {
      const list = map.get(botId)
      if (list) map.set(botId, rename(list))
    }
    this.changed()
    await this.refreshThreads(botId)
    await this.refreshThreads(botId, true)
  }

  async setThreadArchived(botId: string, threadId: string, archived = true) {
    await (await this.ready()).archiveThread(threadId, archived)
    this.threads.set(botId, (this.threads.get(botId) ?? []).filter((t) => t.id !== threadId))
    this.archivedThreads.set(botId, (this.archivedThreads.get(botId) ?? []).filter((t) => t.id !== threadId))
    this.changed()
    await this.refreshThreads(botId)
    await this.refreshThreads(botId, true)
  }

  /** Removes a thread with its lines; the chat keeps everything else. */
  async deleteThread(botId: string, threadId: string) {
    await (await this.ready()).deleteThread(threadId)
    this.threads.set(botId, (this.threads.get(botId) ?? []).filter((t) => t.id !== threadId))
    this.archivedThreads.set(botId, (this.archivedThreads.get(botId) ?? []).filter((t) => t.id !== threadId))
    this.changed()
  }

  /** What a voice call carried, from the computer: the loaded chat may not reach back to it. */
  async callEntries(botId: string, call: string): Promise<Entry[]> {
    return (await (await this.ready()).call<{ entries: Entry[] }>('callEntries', { botId, callId: call })).entries.filter((e) => !isInternalEntry(e))
  }

  /** The history between a bot and one peer. Never upserted: old notices would break `loadOlder`'s paging. */
  async botConversation(botId: string, peerId: string): Promise<Entry[]> {
    return (await (await this.ready()).botConversation(botId, peerId)).filter((e) => !isInternalEntry(e))
  }

  /** What `backend` already has in the user's home, not the Codync skills and connectors a bot passes on. */
  async agentGear(backend: string): Promise<{ skills: AgentGearItem[]; connectors: AgentGearItem[]; rules: AgentGearItem[]; plugins: AgentGearItem[] }> {
    const client = this.client
    if (!client) throw new Error("Couldn't load what this agent brings.")
    const value = await client.call<{ skills?: AgentGearItem[]; connectors?: AgentGearItem[]; rules?: AgentGearItem[]; plugins?: AgentGearItem[] }>('agentGear', { backend })
    return { skills: value.skills ?? [], connectors: value.connectors ?? [], rules: value.rules ?? [], plugins: value.plugins ?? [] }
  }

  async refreshWhatsApp() {
    this.whatsapp = await (await this.ready()).call<import('@shared/models').WhatsAppState>('whatsappState')
    this.changed()
  }
  async whatsappConnect() { return (await this.ready()).call('whatsappConnect') }
  /** Links a number with a pairing code to type in WhatsApp instead of scanning a QR code. */
  async whatsappPair(phone: string) {
    const state = await (await this.ready()).call<import('@shared/models').WhatsAppState>('whatsappPair', { phone })
    this.whatsapp = state
    this.changed()
  }
  async whatsappLogout() { return (await this.ready()).call('whatsappLogout') }
  async whatsappSetBot(botId: string | null) { return this.whatsapp = { ...(this.whatsapp ?? { state: 'disconnected' }), bot: botId }, await (await this.ready()).call('whatsappSetBot', { botId: botId ?? '' }), this.changed() }
  /** Lets a chat or person write, or takes them out; an empty list lets anyone in. */
  async whatsappSetAllowed(jid: string, allowed: boolean) {
    const result = await (await this.ready()).call<{ allowed: string[] }>('whatsappSetAllowed', { jid, allowed })
    this.whatsapp = { ...(this.whatsapp ?? { state: 'disconnected' }), allowed: result.allowed }
    this.changed()
  }

  /** Removes one message on the computer. */
  async deleteEntry(id: string) {
    const client = await this.ready()
    await client.call('deleteEntry', { entryId: id })
    this.removeEntry(id)
    this.changed()
  }

  /** Sends a message on to another chat; the host copies its files and marks it forwarded. */
  async forward(entryId: string, botId: string) {
    await (await this.ready()).call('forward', { entryId, botId })
  }

  /** Turns a model off or back on in the picker. */
  async setModelHidden(backend: string, model: string, hidden: boolean) {
    await (await this.ready()).call('setModelHidden', { backend, model, hidden })
  }

  async save(draft: BotDraft) {
    const client = await this.ready()
    // A rule left blank in the editor is no rule.
    const saved = draft.approvalRules ? { ...draft, approvalRules: cleanRules(draft.approvalRules) } : draft
    const res = draft.id
      ? await client.call<{ bot: Bot }>('updateBot', { ...saved, model: saved.model ?? null, account: saved.account ?? null })
      : await client.call<{ bot: Bot }>('createBot', draft)
    const bot = this.putHostBot(normalizeBot(res.bot))
    this.onRosterChanged?.()
    this.changed()
    return bot
  }

  async refreshBackends() {
    if (!this.client || !this.hello) return
    try {
      const res = await this.client.call<{ backends: Hello['backends'] }>('refreshBackends', {}, 60_000)
      this.hello = { ...this.hello, backends: res.backends }
      this.changed()
    } catch {}
  }

  async sentinelHealth(): Promise<SentinelReport> {
    return (await this.ready()).call<SentinelReport>('sentinelHealth', {})
  }

  async listBotApps(botId: string): Promise<BotApp[]> {
    const res = await (await this.ready()).call<{ apps: BotApp[] }>('listBotApps', { botId })
    return res.apps ?? []
  }

  async createAppLink(botId: string, slug: string, mode: AppLinkMode, label = ''): Promise<CreatedAppLink> {
    return createAppShareLink(await this.ready(), botId, slug, mode, label)
  }

  async revokeAppLink(botId: string, slug: string, token: string): Promise<void> {
    await (await this.ready()).call('revokeAppLink', { botId, slug, token })
  }

  async deleteBotApp(botId: string, slug: string): Promise<void> {
    await (await this.ready()).call('deleteBotApp', { botId, slug })
  }

  async listOpenItems(botId: string): Promise<OpenItem[]> {
    const res = await (await this.ready()).call<{ items: OpenItem[] }>('listOpenItems', { botId })
    return res.items ?? []
  }

  async dismissOpenItem(botId: string, id: string): Promise<void> {
    await (await this.ready()).call('dismissOpenItem', { botId, id })
  }

  async listBookmarks(botId: string): Promise<MessageBookmark[]> {
    const res = await (await this.ready()).call<{ bookmarks: MessageBookmark[] }>('listBookmarks', { botId })
    return res.bookmarks ?? []
  }

  async saveBookmark(botId: string, entryId: string, preview: string, note = '', author = '', threadId?: string | null): Promise<MessageBookmark> {
    const res = await (await this.ready()).call<{ bookmark: MessageBookmark }>('saveBookmark', { botId, entryId, preview, note, author, threadId: threadId ?? null })
    return res.bookmark
  }

  async deleteBookmark(botId: string, entryId: string): Promise<void> {
    await (await this.ready()).call('deleteBookmark', { botId, entryId })
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

  async listDirs(path: string | null) {
    return (await this.ready()).call<DirListing>('listDirs', { path })
  }

  async selectWorkspace(path: string) {
    return (await this.ready()).call<{ path: string }>('selectWorkspace', { path })
  }

  async agentAccounts(backend: string): Promise<AgentAccount[]> {
    const result = await (await this.ready()).call<{ accounts: AgentAccount[] }>('listAccounts', { backend })
    return result.accounts
  }

  /** The keys saved for this bot, and those it inherits from its agent (names only). */
  async agentEnv(backend: string, botId: string): Promise<{ own: string[]; inherited: string[] }> {
    return (await this.ready()).call('agentEnv', { backend, botId })
  }

  /** Saves or removes one of the bot's own agent keys; an empty value removes it. */
  async setAgentEnv(backend: string, vars: Record<string, string>, botId: string) {
    await (await this.ready()).call('setAgentEnv', { backend, vars, botId })
  }

  async authenticateAgentAccount(backend: string): Promise<AgentAccount> {
    const result = await (await this.ready()).call<{ account: AgentAccount }>('accountAuthenticate', { backend }, 660_000)
    return result.account
  }

  async deleteAgentAccount(accountId: string): Promise<void> {
    await (await this.ready()).call('deleteAccount', { accountId }, 30_000)
    await this.refreshBackends()
  }

  /** Removes the computer's own sign-in for an agent, without touching its isolated accounts. */
  async signOutAgent(backend: string): Promise<void> {
    await (await this.ready()).call('agentSignOut', { backend }, 60_000)
    await this.refreshBackends()
  }

  /** The last seven days on this computer; null when the host can't say (an older one). */
  async weekSummary(): Promise<WeekSummary | null> {
    if (this.retired || !this.client) return null
    try {
      return await this.client.call<WeekSummary>('weekSummary', { days: 7 })
    } catch {
      return null
    }
  }

  /** Takes back what the bot just learned (the notice's Undo); the notice itself comes back changed. */
  async undoMemoryChange(entry: Entry) {
    const res = await (await this.ready()).call<{ entry: Entry }>('undoMemoryChange', { botId: entry.botId, entryId: entry.id })
    this.upsert(res.entry)
    this.changed()
  }

  /** Reads the computer's health; an older host keeps what was there (nothing). */
  async refreshHealth() {
    if (this.retired || !this.client) return
    try {
      this.health = await this.client.call<HostHealth>('hostHealth', {})
      this.changed()
    } catch {}
  }

  /** Changes the memory levels that warn and that stop new turns; throws the host's reason when they make no sense. */
  async setHealthThresholds(warn: number, critical: number) {
    if (this.retired || !this.client) return
    this.health = await this.client.call<HostHealth>('setHealthThresholds', { warn, critical })
    this.changed()
  }

  async refreshUsage() {
    if (this.retired || !this.client) return
    try {
      const next = await this.client.call<Usage>('usage', { refresh: true })
      // A probe already in flight answers with the store it had before it
      // started (often empty on launch). Keep what is on screen until a real
      // reading arrives instead of flashing "No usage".
      if (next.providers.length > 0 || this.usage.providers.length === 0) this.usage = next
      this.changed()
    } catch {}
  }

  async loadOlder(botId: string) {
    if (!this.client || this.historyComplete.has(botId)) return
    const first = (this.entries.get(botId) ?? []).find((e) => e.seq > 0 && e.seq !== Number.MAX_SAFE_INTEGER)?.seq ?? Number.MAX_SAFE_INTEGER
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
}

/** A full editor update must explicitly clear a previous model override. */
function updatePayload(bot: Bot): BotDraft {
  return {
    id: bot.id, name: bot.name, description: bot.description, avatarColor: bot.avatarColor, avatarShape: bot.avatarShape,
    avatarLook: bot.avatarLook ?? null, actionRules: bot.actionRules ?? {}, proactive: bot.proactive,
    backend: bot.backend, command: bot.command ?? null, cwd: bot.managedWorkspace ? '' : bot.cwd, permission: bot.permission,
    model: bot.model ?? null, pinned: bot.pinned, hidden: bot.hidden, notify: bot.notify ?? null,
    connectors: bot.connectors, skills: bot.skills, computer: bot.computer, computerPermissions: bot.computerPermissions, fastMode: bot.fastMode,
    isChief: bot.isChief, chiefBotId: bot.chiefBotId ?? null,
  }
}

// ponytail: in-memory attachment cache; a disk cache (main process) if relaunches refetching hurts.
const attachmentCache = new Map<string, Uint8Array>()
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
