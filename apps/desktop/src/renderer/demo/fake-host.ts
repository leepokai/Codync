import type { CallError } from '@shared/ipc'
import { isChat, normalizeBot, type Bot, type BotDraft, type Entry, type GroupDraft } from '@shared/models'
import * as story from './story'

// An in-memory codync-host for the website demo: the methods the renderer calls, with the
// host's JSON shapes (host/src/api/), and the events stream (hello, then catch-up in rev
// order, then live). Every mutation stamps a global rev, like the real host.

const TYPING_MS = 1500

export const DEMO_UNAVAILABLE = 'Not available in the web demo. Install Codync to use this.'

export class FakeHost {
  readonly hostId = 'demo-host'
  private rev = 0
  private seq = 0
  private bots = new Map<string, Bot>()
  private entries: Entry[] = []
  private listeners = new Set<(line: string) => void>()
  private turns = new Map<string, number>()
  private nextReply = new Map<string, number>()
  /** Already chosen, so the demo never asks for consent. */
  private analytics = false

  constructor(
    readonly version: string,
    now = Date.now(),
  ) {
    for (const b of story.bots) {
      this.bots.set(b.id, normalizeBot({ ...b, cwd: b.kind === 'group' ? '' : story.cwd, createdAt: now - 86_400_000, permission: b.permission ?? 'auto' }))
    }
    for (const s of story.entries) this.add({ botId: s.botId, threadId: s.threadId ?? null, kind: s.kind, data: s.data }, s.id, now - s.at * 60_000, false)
    for (const b of this.bots.values()) {
      this.refreshPreview(b)
      b.rev = ++this.rev
    }
  }

  // MARK: events

  /** `GET /events?since=`: hello, everything after `since` in rev order, then live events. */
  subscribe(since: number, onData: (line: string) => void) {
    const catchUp: { rev: number; line: object }[] = [
      ...[...this.bots.values()].filter((b) => b.rev > since).map((b) => ({ rev: b.rev, line: { type: 'bot', rev: b.rev, bot: this.botJSON(b) } })),
      ...this.entries.filter((e) => e.rev > since).map((e) => ({ rev: e.rev, line: { type: 'entry', rev: e.rev, entry: e } })),
    ].sort((a, b) => a.rev - b.rev)
    const hello = { type: 'hello', hostId: this.hostId, computerId: story.COMPUTER_ID, rev: this.rev, usage: story.usage(Date.now()), screen: null }
    let open = true
    const listener = (line: string) => open && onData(line)
    setTimeout(() => {
      if (!open) return
      for (const item of [{ line: hello }, ...catchUp]) onData(JSON.stringify(item.line))
      this.listeners.add(listener)
    }, 0)
    return () => {
      open = false
      this.listeners.delete(listener)
    }
  }

  private emit(event: object) {
    const line = JSON.stringify(event)
    for (const l of this.listeners) l(line)
  }

  private emitBot(bot: Bot) {
    bot.rev = ++this.rev
    this.emit({ type: 'bot', rev: bot.rev, bot: this.botJSON(bot) })
    // A group shows its busy member's status: it changes with them.
    if (bot.kind !== 'group') for (const g of this.bots.values()) if (g.kind === 'group' && g.members.includes(bot.id)) this.emitBot(g)
  }

  private emitEntry(e: Entry) {
    e.rev = ++this.rev
    e.updatedAt = Date.now()
    this.emit({ type: 'entry', rev: e.rev, entry: e })
  }

  /** The host's `bot_json`: a group is busy while one of its members works in it. */
  private botJSON(b: Bot): Bot {
    if (b.kind !== 'group') return b
    const busy = b.members.map((m) => this.bots.get(m)).find((m) => m && m.status !== 'idle' && m.workingChat === b.id)
    return busy
      ? { ...b, status: busy.status, activity: busy.activity, startedAt: busy.startedAt, workingChat: b.id, workingThread: busy.workingThread }
      : b
  }

  // MARK: entries

  private add(e: Pick<Entry, 'botId' | 'threadId' | 'kind' | 'data'>, id: string = crypto.randomUUID(), at = Date.now(), emit = true) {
    const entry: Entry = { ...e, id, seq: ++this.seq, rev: ++this.rev, turn: 1, createdAt: at, updatedAt: at }
    this.entries.push(entry)
    if (entry.threadId) this.summarize(entry.botId, entry.threadId, emit)
    if (emit) {
      this.emit({ type: 'entry', rev: entry.rev, entry })
      const bot = this.bots.get(entry.botId)
      if (bot && !entry.threadId && isChat(entry)) {
        this.refreshPreview(bot)
        if (entry.kind !== 'user') bot.unread++
        this.emitBot(bot)
      }
    }
    return entry
  }

  /** `data.thread` on the root: reply count, last reply, who replied. */
  private summarize(botId: string, root: string, emit: boolean) {
    const rootEntry = this.entries.find((e) => e.id === root)
    if (!rootEntry) return
    const replies = this.entries.filter((e) => e.botId === botId && e.threadId === root && isChat(e))
    const authors: string[] = []
    for (const r of replies) {
      const a = r.kind === 'user' ? 'user' : (r.data.author ?? botId)
      if (!authors.includes(a)) authors.push(a)
    }
    rootEntry.data = { ...rootEntry.data, thread: { count: replies.length, lastAt: replies.at(-1)?.createdAt ?? 0, authors, unread: 0 } }
    if (emit) this.emitEntry(rootEntry)
  }

  private refreshPreview(b: Bot) {
    const last = this.entries.filter((e) => e.botId === b.id && !e.threadId && (e.kind === 'user' || (e.kind === 'agent' && e.data.final))).at(-1)
    if (!last) return
    const text = last.data.text ?? ''
    b.lastAt = last.createdAt
    b.lastMessage = last.kind === 'user' ? `You: ${text}` : b.kind === 'group' ? `${this.bots.get(last.data.author ?? '')?.name ?? 'A bot'}: ${text}` : text
  }

  // MARK: turns

  private setStatus(botId: string, status: string, activity = '', chat: string | null = null, thread: string | null = null) {
    const bot = this.bots.get(botId)
    if (!bot) return
    Object.assign(bot, { status, activity, startedAt: status === 'idle' ? null : Date.now(), workingChat: chat, workingThread: thread })
    this.emitBot(bot)
  }

  private pending(botId: string) {
    return this.entries.find((e) => e.kind === 'permission' && e.data.author === botId && e.data.status === 'pending') ?? null
  }

  private settle(botId: string) {
    const waiting = this.pending(botId)
    if (waiting) this.setStatus(botId, 'needsInput', `Needs approval: ${waiting.data.title ?? ''}`, waiting.botId)
    else this.setStatus(botId, 'idle')
  }

  /** One bot answers in a chat or thread after a short "Typing…". */
  private async answer(responder: string, chat: string, thread: string | null, text: string) {
    const turn = (this.turns.get(responder) ?? 0) + 1
    this.turns.set(responder, turn)
    this.setStatus(responder, 'working', 'Typing…', chat, thread)
    await sleep(TYPING_MS + Math.random() * 600)
    if (this.turns.get(responder) !== turn) return
    this.add({ botId: chat, threadId: thread, kind: 'agent', data: { text, final: true, author: responder } })
    this.settle(responder)
  }

  private replyText(botId: string) {
    if (this.pending(botId)) return story.waitingReply
    const pool = story.replies[botId] ?? ['Got it. I\'ll take a look and report back.']
    const i = this.nextReply.get(botId) ?? 0
    this.nextReply.set(botId, i + 1)
    return pool[i % pool.length]!
  }

  private async converse(chat: Bot, thread: string | null, text: string) {
    let responders = [chat.id]
    if (chat.kind === 'group') {
      const members = chat.members.filter((m) => this.bots.has(m))
      const mentioned = members.filter((m) => text.toLowerCase().includes(`@${this.bots.get(m)!.name.toLowerCase()}`))
      const rootAuthor = thread ? this.entries.find((e) => e.id === thread)?.data.author : undefined
      // A room turn: who was asked, the thread's bot, or the first two members.
      responders = mentioned.length ? mentioned : rootAuthor && members.includes(rootAuthor) ? [rootAuthor] : members.slice(0, 2)
    }
    for (const r of responders) await this.answer(r, chat.id, thread, this.replyText(r))
  }

  // MARK: API

  async call(method: string, b: Record<string, unknown>): Promise<unknown> {
    const str = (k: string) => (typeof b[k] === 'string' ? (b[k] as string) : '')
    switch (method) {
      case 'hello':
        return {
          hostId: this.hostId, computerId: story.COMPUTER_ID, signKey: '', boxKey: '', cloud: null, name: story.HOST_NAME,
          version: this.version, minApp: this.version, os: 'macos', device: 'MacBookPro', home: '/Users/demo', rev: this.rev, analytics: this.analytics,
          urls: [], backends: [
            { id: 'claude', name: 'Claude Code', available: true, installed: true, signedIn: true, command: 'claude', installHint: '' },
            { id: 'codex', name: 'Codex', available: true, installed: true, signedIn: true, command: 'codex', installHint: '' },
          ],
        }
      case 'accessRequests':
        return { requests: [] }
      case 'cloudStatus':
        return { enabled: false }
      case 'usage':
        return story.usage(Date.now())
      case 'history':
        return { entries: [] }
      case 'thread':
        return { entries: this.entries.filter((e) => e.botId === str('botId') && e.threadId === str('rootId')) }
      case 'send': {
        const chat = this.bots.get(str('botId'))
        if (!chat) throw fail('That bot was deleted.')
        const thread = str('threadId') || null
        const entry = this.add({ botId: chat.id, threadId: thread, kind: 'user', data: { text: str('text'), status: 'sent', clientNonce: str('clientNonce') } })
        void this.converse(chat, thread, str('text'))
        return { entry }
      }
      case 'markRead': {
        const bot = this.bots.get(str('botId'))
        if (bot && bot.unread > 0) {
          bot.unread = 0
          this.emitBot(bot)
        }
        return {}
      }
      case 'respondPermission':
        return this.respond(str('entryId'), str('optionId') || null)
      case 'react': {
        const e = this.entries.find((x) => x.id === str('entryId'))
        if (!e) throw fail('That message is gone.')
        const reactions = new Set(e.data.reactions ?? [])
        if (!reactions.delete(str('emoji'))) reactions.add(str('emoji'))
        e.data = { ...e.data, reactions: [...reactions] }
        this.emitEntry(e)
        return { entry: e }
      }
      case 'stop': {
        const id = str('botId')
        this.turns.set(id, (this.turns.get(id) ?? 0) + 1)
        this.settle(id)
        return {}
      }
      case 'newSession':
        this.add({ botId: str('botId'), threadId: null, kind: 'notice', data: { text: 'New session', style: 'divider' } })
        return {}
      case 'createBot':
      case 'updateBot':
        return { bot: this.saveBot(b as unknown as BotDraft | GroupDraft) }
      case 'deleteBot': {
        const id = str('botId')
        this.bots.delete(id)
        this.emit({ type: 'bot', rev: ++this.rev, bot: { id, rev: this.rev, deleted: true } })
        return {}
      }
      case 'refreshBackends':
        return { backends: ((await this.call('hello', {})) as { backends: unknown }).backends }
      case 'listDirs':
        return { path: '/Users/demo/code', parent: '/Users/demo', isGit: false, dirs: [{ name: 'pace', path: story.cwd, isGit: true }] }
      case 'memory':
        return { location: '', facts: [], total: 0, nextOffset: null }
      case 'routines':
        return { routines: [], runs: [] }
      case 'skills':
      case 'connectors':
      case 'marketSkills':
      case 'marketConnectors':
      case 'credentialLogins':
      case 'composioToolkits':
        return { items: [] }
      case 'devices':
        return { devices: [] }
      case 'voiceStatus':
        return { providers: [] }
      case 'composioStatus':
        return { configured: false, keyUrl: '' }
      case 'credentialStatus':
        return { provider: '', onePasswordConnected: false }
      case 'setAnalytics':
        this.analytics = b.enabled === true
        return { enabled: this.analytics }
      case 'track':
        return {}
      default:
        throw fail(DEMO_UNAVAILABLE)
    }
  }

  private async respond(entryId: string, optionId: string | null) {
    const e = this.entries.find((x) => x.id === entryId)
    if (!e || e.data.status !== 'pending') return {}
    const option = e.data.options?.find((o) => o.optionId === optionId)
    const allowed = !!option && option.kind.startsWith('allow')
    e.data = { ...e.data, status: optionId ? 'answered' : 'cancelled', selected: optionId ?? undefined }
    this.emitEntry(e)
    const author = e.data.author ?? e.botId
    void (async () => {
      this.setStatus(author, 'working', allowed ? 'Editing src/pace.js' : 'Typing…', e.botId)
      await sleep(TYPING_MS)
      this.add({ botId: e.botId, threadId: e.threadId ?? null, kind: 'agent', data: { text: allowed ? story.approvedReply : story.deniedReply, final: true, author } })
      this.settle(author)
    })()
    return {}
  }

  private saveBot(draft: BotDraft | GroupDraft) {
    const existing = draft.id ? this.bots.get(draft.id) : undefined
    const bot = normalizeBot({
      ...(existing ?? { createdAt: Date.now(), cwd: 'cwd' in draft ? draft.cwd || story.cwd : '' }),
      ...(draft as Partial<Bot>),
      id: existing?.id ?? crypto.randomUUID(),
    })
    if (existing) Object.assign(existing, bot)
    const saved = existing ?? bot
    this.bots.set(saved.id, saved)
    this.emitBot(saved)
    return this.botJSON(saved)
  }
}

function fail(message: string): CallError {
  return { status: 400, message }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
