// The event box (host/src/chat/tasks/): what a bot works on away from its chat is an item in its box, and a bounded
// number of its agents take items from it. The chat shows one capsule above the composer; the bot's Details lists
// the items. A task is the agent session working an item (`tasks.ts` reads its transcript).

import { fill } from './fill.ts'
import type { Entry } from './models'
import { isTold } from './tasks.ts'

/** The six words for work, the same on the wire and in every client. */
export type WorkStatus = 'waiting' | 'working' | 'needsYou' | 'done' | 'failed' | 'stopped'

const STATUSES: readonly string[] = ['waiting', 'working', 'needsYou', 'done', 'failed', 'stopped'] satisfies WorkStatus[]
const WAITS: readonly string[] = ['next', 'behind', 'after', 'usage'] satisfies WorkWait['kind'][]

/** The most agents a bot may run at once. */
export const MAX_AGENTS = 6
/** How many a bot runs at once unless the user changed it: what a client counts with when the host names none. */
export const DEFAULT_AGENTS = 3

/** Why an item waits: a place to come free, items ahead of it, the item it follows, or the agent's usage limit. */
export interface WorkWait {
  kind: 'next' | 'behind' | 'after' | 'usage'
  /** `behind`: how many are ahead. */
  ahead: number
}

/** Where an item came from when the bot didn't add it itself: a run of the routine `id`, or a request from the bot
 * `id` (null when the asking bot isn't one of this computer's). */
export interface WorkSource {
  kind: 'routine' | 'request'
  id: string | null
}

/** One piece of work in a bot's box (`bot.work`). */
export interface WorkItem {
  id: string
  title: string
  status: WorkStatus
  /** What its task is doing now; empty until it says. */
  activity: string
  /** The task that has or had it; null for one no task has taken. */
  task: string | null
  wait: WorkWait | null
  /** It gave way to another item once, and starts again from its brief. */
  cutIn: boolean
  createdAt: number
  startedAt: number | null
  endedAt: number | null
  /** What its task reported, or why it failed. */
  result: string
  /** Null for one the bot added. */
  source: WorkSource | null
  /** The git worktree its task works in, while there is one: its branch and folder. */
  worktree: { branch: string; path: string } | null
  /** The folder its task works in (the worktree's, for one in a worktree); null until a task took it. */
  folder: string | null
  /** Its thread takes the user's messages, as the host says: the bot's own item whose task still exists. */
  talk: boolean
}

type Translate = (text: string) => string
type Raw = Partial<Omit<WorkItem, 'wait' | 'source' | 'worktree'>> & { worktree?: { branch?: string; path?: string } | null } & { wait?: Partial<WorkWait> | null; source?: { kind?: string; id?: string | null } | null }

/** `bot.work`; a host from before the box sends none, and the feature doesn't show. An item in a status this app
 * doesn't know is left out. */
export function normalizeWork(raw: unknown): WorkItem[] {
  if (!Array.isArray(raw)) return []
  return (raw as (Raw | null)[]).flatMap((i) =>
    i && typeof i.id === 'string' && STATUSES.includes(i.status ?? '')
      ? [{
          id: i.id,
          title: i.title ?? '',
          status: i.status as WorkStatus,
          activity: i.activity ?? '',
          task: i.task ?? null,
          wait: i.wait && WAITS.includes(i.wait.kind ?? '') ? { kind: i.wait.kind as WorkWait['kind'], ahead: i.wait.ahead ?? 1 } : null,
          cutIn: i.cutIn ?? false,
          createdAt: i.createdAt ?? 0,
          startedAt: i.startedAt ?? null,
          endedAt: i.endedAt ?? null,
          result: i.result ?? '',
          // A source this app doesn't know reads as the bot's own.
          worktree: i.worktree?.branch ? { branch: i.worktree.branch, path: i.worktree.path ?? '' } : null,
          source: i.source?.kind === 'routine' || i.source?.kind === 'request' ? { kind: i.source.kind, id: i.source.id ?? null } : null,
          folder: i.folder || null,
          talk: i.talk ?? false,
        }]
      : [],
  )
}

/** `bot.maxAgents`, within 1 to 6; null from a host before the box. */
export const normalizeMaxAgents = (raw: unknown): number | null =>
  typeof raw === 'number' ? Math.min(MAX_AGENTS, Math.max(1, Math.round(raw))) : null

/** A task has it: working, or waiting on the user's approval. */
export const isLive = (item: WorkItem) => item.status === 'working' || item.status === 'needsYou'

/** Still in the box's hands: a task has it, or it waits for one. */
export const isOpen = (item: WorkItem) => isLive(item) || item.status === 'waiting'

/** All bots' work in one view: the bots of `roster` (as its rows are on screen) that have a live or waiting item, in that order, and how many
 * items are worked on in all (the count on the view's button). */
export function allWork<T extends { bot: { work: WorkItem[] } }>(roster: T[]): { bots: T[]; working: number } {
  const bots = roster.filter((r) => r.bot.work.some(isOpen))
  return { bots, working: bots.reduce((sum, r) => sum + r.bot.work.filter((i) => i.status === 'working').length, 0) }
}

/** How a bot stands outside its chat, as the host says it (`bot.outside`); null from a host before it. */
export interface OutsideStatus {
  /** idle | working | needsInput | error, or a newer one. */
  status: string
  startedAt: number | null
}

export const normalizeOutside = (raw: unknown): OutsideStatus | null => {
  const outside = raw as Partial<OutsideStatus> | null | undefined
  return outside && typeof outside.status === 'string' ? { status: outside.status, startedAt: outside.startedAt ?? null } : null
}

/** The status a bot has for what shows it outside its chat (its avatar, the tray, the notch island): it needs the
 * user while its chat or one of its items does, it works while its chat or its box does, else the chat's own status.
 * The host's word (`bot.outside`); the chat's own status from a host before it, as on every client. */
export function outsideStatus(bot: { status: string; outside: OutsideStatus | null }): string {
  return bot.outside?.status ?? bot.status
}

/** Working or waiting on the user, outside its chat: the tray's working mark. */
export function worksOutside(bot: { status: string; outside: OutsideStatus | null }): boolean {
  const status = outsideStatus(bot)
  return status === 'working' || status === 'needsInput'
}

/** The Details timeline's sections of items, each in the order its rows show. */
export interface WorkSections {
  /** Threads, the agent sessions that run now: items that need the user, then the working ones. */
  threads: WorkItem[]
  /** The most threads allowed (the heading's `2 / 3`; `2 / 1` right after the ceiling was lowered). */
  places: number
  /** The places not in use, a `Free` row each. */
  free: number
  /** The event box, what waits for a thread: waiting items in the order they will be taken. */
  box: WorkItem[]
  /** Items that ended in the last day, newest first. */
  recent: WorkItem[]
}

export function workSections(work: WorkItem[], maxAgents: number | null): WorkSections {
  const of = (status: WorkStatus) => work.filter((i) => i.status === status)
  const threads = [...of('needsYou'), ...of('working')]
  const places = maxAgents ?? DEFAULT_AGENTS
  return {
    threads,
    places,
    free: Math.max(0, places - threads.length),
    box: of('waiting'),
    recent: work.filter((i) => i.status === 'done' || i.status === 'failed' || i.status === 'stopped'),
  }
}

const counted = (text: string, count: number, tr: Translate) => tr(text).replaceAll('{count}', () => String(count))

/** `2 working · 3 waiting`, a part left out at none; empty with neither. */
export function workCounts(work: WorkItem[], tr: Translate): string {
  const working = work.filter((i) => i.status === 'working').length
  const waiting = work.filter((i) => i.status === 'waiting').length
  return [working ? counted('{count} working', working, tr) : '', waiting ? counted('{count} waiting', waiting, tr) : ''].filter(Boolean).join(' · ')
}

/** What the capsule says after `In the background ·`. */
export interface CapsuleLabel {
  /** What leads it: an item waits on the user (the text starts with that), some item works, or all of them wait. */
  mark: 'needsYou' | 'working' | 'waiting'
  text: string
  /** When the one working item started: the capsule's time. Null when it counts items instead. */
  since: number | null
  /** The one item a task has, which the capsule's Stop stops; null with none or several. */
  stops: WorkItem | null
}

/** The capsule while any item is live or waiting; null otherwise. */
export function capsuleLabel(work: WorkItem[], tr: Translate): CapsuleLabel | null {
  const live = work.filter(isLive)
  const waiting = work.filter((i) => i.status === 'waiting')
  if (live.length + waiting.length === 0) return null
  const stops = live.length === 1 ? live[0]! : null
  if (live.some((i) => i.status === 'needsYou')) {
    return { mark: 'needsYou', text: [tr('Needs you'), workCounts(work, tr)].filter(Boolean).join(' · '), since: null, stops }
  }
  if (stops && waiting.length === 0) return { mark: 'working', text: stops.activity ? tr(stops.activity) : stops.title, since: stops.startedAt, stops }
  return { mark: live.length ? 'working' : 'waiting', text: workCounts(work, tr), since: null, stops }
}

/** A roster row's line while something goes on, the most pressing first: it needs the user, what its chat does, how
 * many items work and wait. Null when the row reads as usual (its last message). */
export interface RosterLine {
  kind: 'needsYou' | 'working' | 'background'
  text: string
}

export function rosterLine(bot: { status: string; activity: string; work: WorkItem[] }, tr: Translate): RosterLine | null {
  if (bot.status === 'needsInput') return { kind: 'needsYou', text: tr(bot.activity || 'Needs approval') }
  if (bot.work.some((i) => i.status === 'needsYou')) return { kind: 'needsYou', text: tr('Needs you') }
  if (bot.status === 'working') return { kind: 'working', text: tr(bot.activity || 'Working…') }
  const counts = workCounts(bot.work, tr)
  return counts ? { kind: 'background', text: counts } : null
}

/** A bot's line outside the chat window (the tray menu, the notch island), in the roster row's order: it needs the
 * user, what its chat does, how many items work and wait, then its last message. Null with none of them. */
export function summaryLine(bot: { status: string; activity: string; work: WorkItem[]; lastMessage?: string | null }, tr: Translate): string | null {
  if (bot.status === 'needsInput') return tr('Needs your response')
  return rosterLine(bot, tr)?.text ?? bot.lastMessage ?? null
}

/** An item's status in a word. */
export function workStatusWord(status: WorkStatus): string {
  switch (status) {
    case 'waiting':
      return 'Waiting'
    case 'working':
      return 'Working'
    case 'needsYou':
      return 'Needs you'
    case 'done':
      return 'Done'
    case 'failed':
      return 'Failed'
    case 'stopped':
      return 'Stopped'
  }
}

/** Why a waiting item waits. */
export function waitText(item: WorkItem, tr: Translate): string {
  if (item.cutIn) return tr('Was cut in on; starts again')
  switch (item.wait?.kind) {
    case 'next':
      return tr('Next up')
    case 'behind':
      return counted('{count} ahead', item.wait.ahead, tr)
    case 'after':
      return tr('Follows what it works on')
    case 'usage':
      return tr('Near the usage limit: one at a time')
    default:
      return tr('Waiting')
  }
}

/** An item's second line: why it waits, what its task does, or how it ended (a failed or stopped one says why). */
export function itemLine(item: WorkItem, tr: Translate): string {
  switch (item.status) {
    case 'waiting':
      return waitText(item, tr)
    case 'working':
      return tr(item.activity || 'Working')
    case 'failed':
    case 'stopped':
      return item.result || tr(workStatusWord(item.status))
    default:
      return tr(workStatusWord(item.status))
  }
}

/** Try again is offered on an item that failed or was stopped, unless another bot asked for it: that bot has its
 * answer already. A routine's item runs the routine again. */
export const canRetry = (item: WorkItem) => (item.status === 'failed' || item.status === 'stopped') && item.source?.kind !== 'request'

/** A task has it and it may give way to an item that cuts in: only work the bot added. A routine's run or another
 * bot's request is stopped, never set aside. */
export const givesWay = (item: WorkItem) => isLive(item) && item.source === null

/** Every place is taken, so the item first in line can only start now if a task gives way, and one may. Not one that
 * follows unfinished work or waits for usage: a freed place wouldn't start it. */
export function canCutIn(work: WorkItem[], maxAgents: number | null): boolean {
  const first = work.find((i) => i.status === 'waiting')
  const held = first?.wait?.kind === 'after' || first?.wait?.kind === 'usage'
  return !!first && !held && work.some(givesWay) && work.filter(isLive).length >= (maxAgents ?? DEFAULT_AGENTS)
}

/** A card of a task's waiting in the chat for the user: an approval, a choice, or a sign-in its agent needs. */
function isPendingCard(e: Entry): boolean {
  const request = e.data.connectionRequest
  if (request) return request.kind === 'agent' && request.status === 'pending'
  return (e.kind === 'permission' || !!e.data.decision) && e.data.status === 'pending'
}

/** A sign-in card that holds messages: signing in there is what sends them again. */
function holdsMessages(e: Entry): boolean {
  const held = e.data.connectionRequest?.entryIds
  return Array.isArray(held) && held.length > 0
}

/** The card each item that needs the user waits on in the chat, by item id: its task's newest pending one, or, when
 * that is a sign-in and another (the agent the turn left signed out, while its fallback was too) holds the item's
 * message, that one, whose sign-in sends it again. */
export function pendingCards(entries: Entry[]): Map<string, Entry> {
  const newest = new Map<string, Entry>()
  const holding = new Map<string, Entry>()
  for (const e of entries) {
    const item = e.data.item
    if (!item || !isPendingCard(e)) continue
    newest.set(item, e)
    if (holdsMessages(e)) holding.set(item, e)
  }
  return new Map([...newest].map(([item, e]) => [item, (e.data.connectionRequest && holding.get(item)) || e]))
}

/** What each item that needs the user is asked, by item id: what its card waiting in the chat (`pendingCards`) says
 * (an approval's title, a choice's question, which agent to sign in to). */
export function pendingAsks(entries: Entry[], tr: Translate): Map<string, string> {
  const ask = (e: Entry) => {
    const request = e.data.connectionRequest
    return request ? fill(tr('Needs {agent} sign-in'), { agent: String(request.title ?? '') }) : (e.data.title ?? e.data.question ?? '')
  }
  return new Map([...pendingCards(entries)].map(([item, e]) => [item, ask(e)]))
}

/** A task's entry its thread shows as the chat's message bubble: a final reply (text, a file, an artifact card), as
 * the chat tells them apart. What it says on the way is a step. */
export const isThreadMessage = (e: Entry) => e.kind === 'agent' && e.data.final === true

/** One quiet line of a thread for what its task put in the bot's chat. */
export interface SharedLine {
  symbol: string
  text: string
}

/** What the chat entry `e` a task put in the bot's chat (a card, its agent's sign-in card, a file, a voice note, an
 * artifact) says in that task's thread: one line with a symbol, where the entry itself stays in the chat, which the
 * line opens. Null for anything else. */
export function sharedLine(e: Entry, tr: Translate): SharedLine | null {
  const d = e.data
  const line = (symbol: string, key: string, values: Record<string, string> = {}) => ({ symbol, text: fill(tr(key), values) })
  // A sign-in reads as its card does: the needs-you row's words while it waits, then the card's own.
  const request = d.connectionRequest
  if (request?.kind === 'agent') {
    if (request.status === 'pending') return line('hand.raised', 'Needs {agent} sign-in', { agent: String(request.title ?? '') })
    return request.status === 'ready' ? line('checkmark', 'Signed in') : line('xmark', 'Cancelled')
  }
  if (e.kind === 'permission') {
    const title = d.title ?? ''
    if (d.status === 'pending') return line('hand.raised', 'Waiting for your approval: {title}', { title })
    if (d.status !== 'answered') return line('clock', 'Not answered: {text}', { text: title })
    const chosen = d.options?.find((o) => o.optionId === d.selected)
    if (chosen?.kind.startsWith('allow')) return line('checkmark', 'Approved: {title}', { title })
    if (chosen?.kind.startsWith('reject')) return line('xmark', 'Denied: {title}', { title })
    return line('checkmark', 'Answered: {choice}', { choice: chosen?.name ?? d.selected ?? '' })
  }
  if (d.decision) {
    const question = d.question ?? ''
    if (d.status === 'pending') return line('hand.raised', 'Asked: {question}', { question })
    if (d.status === 'answered') return line('checkmark', 'Answered: {choice}', { choice: d.selected ?? '' })
    return line('clock', 'Not answered: {text}', { text: question })
  }
  if (e.kind !== 'agent') return null
  if (d.artifact) return line('doc.richtext', 'Made an artifact: {title}', { title: d.artifact.title })
  if (d.voiceNote) return line('waveform', 'Sent a voice note')
  if (d.files?.length) return line('doc', 'Sent a file: {name}', { name: d.files.map((f) => f.name).join(', ') })
  return null
}

/** Why a thread that takes no message is read only, as a catalog key: its result has an owner already. Null for an
 * item that takes messages, and for the bot's own (nothing to explain). */
export function readOnlyReason(item: Pick<WorkItem, 'talk' | 'source'>): string | null {
  if (item.talk) return null
  switch (item.source?.kind) {
    case 'routine':
      return "Read only: a routine's run"
    case 'request':
      return "Read only: another bot's request"
    default:
      return null
  }
}

/** Where an item's task works, in one quiet line. */
export interface WhereLine {
  kind: 'folder' | 'branch'
  text: string
}

/** The branch when the item has a worktree of its own, else its folder with the home folder as `~`. Null for an item
 * no task has taken. */
export function whereLine(item: Pick<WorkItem, 'folder' | 'worktree'>, home: string): WhereLine | null {
  if (item.worktree) return { kind: 'branch', text: item.worktree.branch }
  return item.folder ? { kind: 'folder', text: homeRelative(item.folder, home) } : null
}

/** The full path the where line copies: the task's folder, or its worktree's from a host that names no folder. */
export const wherePath = (item: Pick<WorkItem, 'folder' | 'worktree'>): string | null => item.folder ?? (item.worktree?.path || null)

/** `path` with the home folder as `~`, on either kind of separator; unchanged outside the home folder or without one. */
function homeRelative(path: string, home: string): string {
  const root = home.replace(/[\\/]+$/, '')
  if (!root || !path.startsWith(root)) return path
  const rest = path.slice(root.length)
  return rest === '' || /^[\\/]/.test(rest) ? `~${rest}` : path
}

/** What Settings says of the bot's folder: git isn't on the computer, the folder is in a repository, or it isn't. */
export type GitState = 'repo' | 'none' | 'missing'

export function gitState(bot: { gitRepo: boolean; gitInstalled: boolean }): GitState {
  if (!bot.gitInstalled) return 'missing'
  return bot.gitRepo ? 'repo' : 'none'
}

/** A thread's transcript as its panel shows it: what the host told the task first (`told`, folded at the top), then
 * the rest in order, without the messages the user took back. What the task `taskId` put in the bot's `chat` (a
 * file, a voice note, an artifact, a card, answered or not) joins the rest where it came, each a `sharedLine`. */
export function threadEntries(entries: Entry[], chat: Entry[] = [], taskId = ''): { told: Entry | null; rest: Entry[] } {
  const shown = entries.filter((e) => !e.data.removed)
  const told = shown.find(isTold) ?? null
  const shared = chat.filter((e) => e.data.task?.id === taskId && !e.data.removed)
  return { told, rest: byTime(shown.filter((e) => e !== told), shared) }
}

/** `own` and `shared`, each kept in its order, as one list in time order (`createdAt`); at the same moment `own` first. */
function byTime(own: Entry[], shared: Entry[]): Entry[] {
  const out: Entry[] = []
  let next = 0
  for (const e of own) {
    while (next < shared.length && shared[next]!.createdAt < e.createdAt) out.push(shared[next++]!)
    out.push(e)
  }
  return out.concat(shared.slice(next))
}
