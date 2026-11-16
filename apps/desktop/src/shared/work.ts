// The event box (host/src/chat/tasks/): what a bot works on away from its chat is an item in its box, and a bounded
// number of its agents take items from it. The chat shows one capsule above the composer; the bot's Details lists
// the items. A task is the agent session working an item (`tasks.ts` reads its transcript).

import type { Entry } from './models'

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
  if (bot.status === 'needsInput') return { kind: 'needsYou', text: tr(bot.activity || 'Needs your approval') }
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
      return tr('Waiting for usage')
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

/** What each item that needs the user is asked, by item id: what its task's card waiting in the chat says (an
 * approval's title, a choice's question). */
export function pendingAsks(entries: Entry[]): Map<string, string> {
  const isCard = (e: Entry) => e.kind === 'permission' || !!e.data.decision
  return new Map(entries.flatMap((e) => (isCard(e) && e.data.status === 'pending' && e.data.item ? [[e.data.item, e.data.title ?? e.data.question ?? ''] as const] : [])))
}
