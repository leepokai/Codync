// The event box (host/src/chat/work/): what a bot works on away from its chat is a task in its box, and a bounded
// number of its threads take tasks from it. The chat shows one capsule above the composer; the bot's Details lists
// the tasks. A thread is where a task is worked, with its own session (`threads.ts` reads its transcript).

import { fill } from './fill.ts'
import type { Entry } from './models'
import { isTold } from './threads.ts'

/** The six words for work, the same on the wire and in every client. */
export type WorkStatus = 'waiting' | 'working' | 'needsYou' | 'done' | 'failed' | 'stopped'

const STATUSES: readonly string[] = ['waiting', 'working', 'needsYou', 'done', 'failed', 'stopped'] satisfies WorkStatus[]
const WAITS: readonly string[] = ['next', 'behind', 'after', 'usage'] satisfies WorkWait['kind'][]

/** The most threads a bot may run at once. */
export const MAX_THREADS = 6
/** How many a bot runs at once unless the user changed it: what a client counts with when the host names none. */
export const DEFAULT_THREADS = 3

/** Why a task waits: a thread to come free, tasks ahead of it, the task it follows, or the agent's usage limit. */
export interface WorkWait {
  kind: 'next' | 'behind' | 'after' | 'usage'
  /** `behind`: how many are ahead. */
  ahead: number
}

/** Where a task came from when the bot didn't add it itself: a run of the routine `id`, or a request from the bot
 * `id` (null when the asking bot isn't one of this computer's). */
export interface WorkSource {
  kind: 'routine' | 'request'
  id: string | null
}

/** One piece of work in a bot's box (`bot.work`). */
export interface WorkTask {
  id: string
  title: string
  status: WorkStatus
  /** What its thread is doing now; empty until it says. */
  activity: string
  /** The thread that has or had it; null for one no thread has taken. */
  thread: string | null
  wait: WorkWait | null
  /** It gave way to another task once, and starts again from its brief. */
  cutIn: boolean
  createdAt: number
  startedAt: number | null
  endedAt: number | null
  /** What its thread reported, or why it failed. */
  result: string
  /** Null for one the bot added. */
  source: WorkSource | null
  /** The git worktree its thread works in, while there is one: its branch and folder. */
  worktree: { branch: string; path: string } | null
  /** The folder its thread works in (the worktree's, for one in a worktree); null until a thread took it. */
  folder: string | null
  /** Its thread takes the user's messages, as the host says: the bot's own task whose thread still exists. */
  talk: boolean
  /** The agent a thread the user started runs on, where it isn't its bot's (always without a bot). */
  agent: string | null
  /** Its thread takes files with the user's messages too; false from a host before them, where its box has no +. */
  talkFiles: boolean
  /** It takes one of the bot's seats, as the host says (one waiting on the user gives its seat up); null from an older
   * host, where every live task takes one. */
  seat: boolean | null
  /** The agent its thread runs on now, or ran on last (a backend id); null before a thread took it, and from an older host. */
  runsOn: string | null
  /** What its thread's agent runs in the background (a command, a workflow, a monitor), oldest first: the task waits
   * on it. Empty from an older host, and for an agent that doesn't say. */
  background: BackgroundTask[]
}

/** A piece of work a thread's agent reports running in the background. */
export interface BackgroundTask {
  id: string
  /** What the agent calls it: a workflow's name, a command's description. */
  name: string
  /** What it does now, when the agent says (a workflow's phase and agent). */
  activity: string | null
  startedAt: number
}

/** The agent a task's thread runs on, or will: its own, else the one it was started on, else its bot's (none for the
 * box of threads without a bot). */
export const agentOf = (task: WorkTask, bot: { backend: string; plain?: boolean }) =>
  task.runsOn ?? task.agent ?? (bot.plain ? null : bot.backend)

export type ThreadPlace = 'here' | 'worktree'

/** What a thread the user starts changes from its bot's setup, for its own task (`startThread`'s `setup`): the folder
 * it works in, its model with that model's settings, and how it is approved. What it leaves out is the bot's. */
export interface ThreadSetup {
  /** The agent: a thread without a bot names it. */
  backend?: string
  /** The agent's login profile, `''` its default login; left out, the bot's. */
  loginProfile?: string
  cwd?: string
  /** In a git project: its own worktree or the project itself; left out, the bot's worktree mode decides. */
  place?: ThreadPlace
  model?: { id: string | null; options: Record<string, string> }
  approval?: { permission: string; mode: string | null }
  /** A thread opened beside a workflow to build it by talking: its workflow tools reach that one only. */
  workflow?: string
}

type Translate = (text: string) => string
type Raw = Partial<Omit<WorkTask, 'wait' | 'source' | 'worktree' | 'background'>> & { background?: unknown } & { worktree?: { branch?: string; path?: string } | null } & { wait?: Partial<WorkWait> | null; source?: { kind?: string; id?: string | null } | null }

/** `bot.work`; a host from before the box sends none, and the feature doesn't show. A task in a status this app
 * doesn't know is left out. */
export function normalizeWork(raw: unknown): WorkTask[] {
  if (!Array.isArray(raw)) return []
  return (raw as (Raw | null)[]).flatMap((t) =>
    t && typeof t.id === 'string' && STATUSES.includes(t.status ?? '')
      ? [{
          id: t.id,
          title: t.title ?? '',
          status: t.status as WorkStatus,
          activity: t.activity ?? '',
          thread: t.thread ?? null,
          wait: t.wait && WAITS.includes(t.wait.kind ?? '') ? { kind: t.wait.kind as WorkWait['kind'], ahead: t.wait.ahead ?? 1 } : null,
          cutIn: t.cutIn ?? false,
          createdAt: t.createdAt ?? 0,
          startedAt: t.startedAt ?? null,
          endedAt: t.endedAt ?? null,
          result: t.result ?? '',
          // A source this app doesn't know reads as the bot's own.
          worktree: t.worktree?.branch ? { branch: t.worktree.branch, path: t.worktree.path ?? '' } : null,
          source: t.source?.kind === 'routine' || t.source?.kind === 'request' ? { kind: t.source.kind, id: t.source.id ?? null } : null,
          folder: t.folder || null,
          talk: t.talk ?? false,
          agent: typeof t.agent === 'string' ? t.agent : null,
          talkFiles: t.talkFiles ?? false,
          seat: typeof t.seat === 'boolean' ? t.seat : null,
          runsOn: typeof t.runsOn === 'string' && t.runsOn ? t.runsOn : null,
          background: normalizeBackground(t.background),
        }]
      : [],
  )
}

function normalizeBackground(raw: unknown): BackgroundTask[] {
  if (!Array.isArray(raw)) return []
  return (raw as (Partial<BackgroundTask> | null)[]).flatMap((t) =>
    t && typeof t.id === 'string'
      ? [{
          id: t.id,
          name: t.name ?? '',
          activity: t.activity || null,
          startedAt: t.startedAt ?? 0,
        }]
      : [],
  )
}

/** The capsule over a thread's message box while its agent runs work in the background: what the one piece does, or
 * the names of several, since the oldest began. Null while nothing runs there. */
export function backgroundLabel(task: WorkTask): { text: string; since: number } | null {
  const [first] = task.background
  if (!first) return null
  const text = task.background.length === 1
    ? [first.name, first.activity].filter(Boolean).join(' · ')
    : task.background.map((t) => t.name).join(', ')
  return { text, since: first.startedAt }
}

/** `bot.maxThreads`, within 1 to 6; null from a host before the box. */
export const normalizeMaxThreads = (raw: unknown): number | null =>
  typeof raw === 'number' ? Math.min(MAX_THREADS, Math.max(1, Math.round(raw))) : null

/** A thread has it: working, or waiting on the user's approval. */
export const isLive = (task: WorkTask) => task.status === 'working' || task.status === 'needsYou'

/** How the Threads page groups what doesn't need the user: under the folder (or routine) it works for, under its
 * bot, under its computer, or not at all. */
export type ThreadGrouping = 'folder' | 'bot' | 'computer' | 'none'

/** How the Threads list orders its rows inside each group: working ones first (`status`), and by when each last moved
 * (`active`: ended, else started, else added) instead of when it was added. Both can be on. */
export interface ThreadSort {
  status?: boolean
  active?: boolean
}

/** Where a thread stands in a list sorted by status: working, then waiting for a thread, then the ended ones. */
const threadRank = (status: WorkStatus) => (status === 'working' ? 0 : status === 'waiting' ? 1 : 2)
/** When a thread last moved: it ended, else it started, else it was added. */
const lastActive = (t: WorkTask) => t.endedAt ?? t.startedAt ?? t.createdAt

/** It takes one of the bot's seats (`maxThreads`): the host's word, else a live task. */
export const takesSeat = (task: WorkTask) => task.seat ?? isLive(task)

/** Still in the box's hands: a thread has it, or it waits for one. */
export const isOpen = (task: WorkTask) => isLive(task) || task.status === 'waiting'

export interface ThreadRowOf<T> {
  of: T
  task: WorkTask
}

/** One group of the Threads page: a folder a thread works in outside its bot's own, a routine's runs, a bot, or a
 * computer. */
export interface ThreadGroup<T> {
  key: string
  kind: 'folder' | 'routine' | 'bot' | 'computer'
  /** The folder's last name, the routine's, the bot's or the computer's. */
  title: string
  rows: ThreadRowOf<T>[]
}

/**
 * All bots' work as the Threads page lists it, newest first (or as `sort` says, inside each group): what needs the user on top, then the groups `grouping`
 * makes (a folder a thread works in outside its bot's own, a worktree of it being the bot's; a routine's runs; or a
 * group per bot, in `roster`'s order, which is the bot list's; or a group per computer, `computerOf` naming it, in
 * the order the computers come), then the rest. `keyOf` tells the bots apart. `working` is how many tasks are worked on in all (the count on the rail's Threads).
 */
export function threadSections<T extends { bot: { work: WorkTask[]; name: string; cwd?: string } }>(roster: T[], grouping: ThreadGrouping, keyOf: (of: T) => string, computerOf: (of: T) => string = () => '', sort: ThreadSort = {}): {
  needsYou: ThreadRowOf<T>[]
  groups: ThreadGroup<T>[]
  recent: ThreadRowOf<T>[]
  working: number
} {
  const time = sort.active ? lastActive : (t: WorkTask) => t.createdAt
  const newest = (a: ThreadRowOf<T>, b: ThreadRowOf<T>) =>
    (sort.status ? threadRank(a.task.status) - threadRank(b.task.status) : 0) || time(b.task) - time(a.task)
  const rows = roster.flatMap((of) => of.bot.work.map((task) => ({ of, task })))
  const needsYou = rows.filter((r) => r.task.status === 'needsYou').sort(newest)
  const rest = rows.filter((r) => r.task.status !== 'needsYou').sort(newest)
  const groups = new Map<string, ThreadGroup<T>>()
  const recent: ThreadRowOf<T>[] = []
  for (const row of rest) {
    const { of, task } = row
    const path = wherePath(task)
    const group: Omit<ThreadGroup<T>, 'rows'> | null =
      grouping === 'bot' ? { key: `bot:${keyOf(of)}`, kind: 'bot', title: of.bot.name }
      : grouping === 'computer' ? { key: `computer:${computerOf(of)}`, kind: 'computer', title: computerOf(of) }
      : grouping === 'none' ? null
      : task.source?.kind === 'routine' ? { key: `routine:${keyOf(of)}:${task.source.id ?? task.title}`, kind: 'routine', title: task.title }
      : path && !task.worktree && path !== of.bot.cwd ? { key: `folder:${path}`, kind: 'folder', title: path.split(/[\\/]/).filter(Boolean).pop() ?? path }
      : null
    if (!group) recent.push(row)
    else (groups.get(group.key) ?? groups.set(group.key, { ...group, rows: [] }).get(group.key)!).rows.push(row)
  }
  // A bot's group keeps the bot list's order, a computer's the computers'; the others come newest first, as their rows do.
  const ordered = grouping === 'bot' ? roster.map((of) => `bot:${keyOf(of)}`) : grouping === 'computer' ? roster.map((of) => `computer:${computerOf(of)}`) : null
  const byBot = ordered ? [...new Set(ordered)].flatMap((key) => groups.get(key) ?? []) : [...groups.values()]
  return { needsYou, groups: byBot, recent, working: rows.filter((r) => r.task.status === 'working').length }
}

/** Where a bot stands in a list sorted by status: it needs the user, it works, it has work in the background, the rest. */
export function rosterRank(bot: { status: string; activity: string; work: WorkTask[] }): number {
  const kind = rosterLine(bot, (text) => text)?.kind
  return kind === 'needsYou' ? 0 : kind === 'working' ? 1 : kind === 'work' ? 2 : 3
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
 * user while its chat or one of its tasks does, it works while its chat or its box does, else the chat's own status.
 * The host's word (`bot.outside`); the chat's own status from a host before it, as on every client. */
export function outsideStatus(bot: { status: string; outside: OutsideStatus | null }): string {
  return bot.outside?.status ?? bot.status
}

/** Working or waiting on the user, outside its chat: the tray's working mark. */
export function worksOutside(bot: { status: string; outside: OutsideStatus | null }): boolean {
  const status = outsideStatus(bot)
  return status === 'working' || status === 'needsInput'
}

/** The Details timeline's sections of tasks, each in the order its rows show. */
export interface WorkSections {
  /** The threads that run now: tasks that need the user, then the working ones. */
  threads: WorkTask[]
  /** The seats taken (the heading's `2` of `2 / 3`): a thread waiting on the user takes none. */
  inUse: number
  /** The most threads allowed (the heading's `3` of `2 / 3`; `2 / 1` right after the ceiling was lowered). */
  atOnce: number
  /** The threads not in use, a `Free` row each. */
  free: number
  /** The event box, what waits for a thread: waiting tasks in the order they will be taken. */
  box: WorkTask[]
  /** Tasks that ended in the last day, newest first. */
  recent: WorkTask[]
}

export function workSections(work: WorkTask[], maxThreads: number | null): WorkSections {
  const of = (status: WorkStatus) => work.filter((t) => t.status === status)
  const threads = [...of('needsYou'), ...of('working')]
  const atOnce = maxThreads ?? DEFAULT_THREADS
  const inUse = work.filter(takesSeat).length
  return {
    threads,
    inUse,
    atOnce,
    free: Math.max(0, atOnce - inUse),
    box: of('waiting'),
    recent: work.filter((t) => t.status === 'done' || t.status === 'failed' || t.status === 'stopped'),
  }
}

const counted = (text: string, count: number, tr: Translate) => tr(text).replaceAll('{count}', () => String(count))

/** `2 working · 3 waiting`, a part left out at none; empty with neither. */
export function workCounts(work: WorkTask[], tr: Translate): string {
  const working = work.filter((t) => t.status === 'working').length
  const waiting = work.filter((t) => t.status === 'waiting').length
  return [working ? counted('{count} working', working, tr) : '', waiting ? counted('{count} waiting', waiting, tr) : ''].filter(Boolean).join(' · ')
}

/** What the capsule says after `Work ·`. */
export interface CapsuleLabel {
  /** What leads it: a task waits on the user (the text starts with that), some task works, or all of them wait. */
  mark: 'needsYou' | 'working' | 'waiting'
  text: string
  /** When the one working task started: the capsule's time. Null when it counts tasks instead. */
  since: number | null
  /** The one task a thread has, which the capsule's Stop stops; null with none or several. */
  stops: WorkTask | null
}

/** The capsule while any task is live or waiting; null otherwise. */
export function capsuleLabel(work: WorkTask[], tr: Translate): CapsuleLabel | null {
  const live = work.filter(isLive)
  const waiting = work.filter((t) => t.status === 'waiting')
  if (live.length + waiting.length === 0) return null
  const stops = live.length === 1 ? live[0]! : null
  if (live.some((t) => t.status === 'needsYou')) {
    return { mark: 'needsYou', text: [tr('Needs you'), workCounts(work, tr)].filter(Boolean).join(' · '), since: null, stops }
  }
  if (stops && waiting.length === 0) return { mark: 'working', text: stops.activity ? tr(stops.activity) : stops.title, since: stops.startedAt, stops }
  return { mark: live.length ? 'working' : 'waiting', text: workCounts(work, tr), since: null, stops }
}

/** A roster row's line while something goes on, the most pressing first: it needs the user, what its chat does, how
 * many tasks work and wait. Null when the row reads as usual (its last message). */
export interface RosterLine {
  kind: 'needsYou' | 'working' | 'work'
  text: string
}

export function rosterLine(bot: { status: string; activity: string; work: WorkTask[] }, tr: Translate): RosterLine | null {
  if (bot.status === 'needsInput') return { kind: 'needsYou', text: tr(bot.activity || 'Needs approval') }
  if (bot.work.some((t) => t.status === 'needsYou')) return { kind: 'needsYou', text: tr('Needs you') }
  if (bot.status === 'working') return { kind: 'working', text: tr(bot.activity || 'Working…') }
  const counts = workCounts(bot.work, tr)
  return counts ? { kind: 'work', text: counts } : null
}

/** A bot's line outside the chat window (the tray menu, the notch island), in the roster row's order: it needs the
 * user, what its chat does, how many tasks work and wait, then its last message. Null with none of them. */
export function summaryLine(bot: { status: string; activity: string; work: WorkTask[]; lastMessage?: string | null }, tr: Translate): string | null {
  if (bot.status === 'needsInput') return tr('Needs your response')
  return rosterLine(bot, tr)?.text ?? bot.lastMessage ?? null
}

/** A task's status in a word. */
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

/** Why a waiting task waits. */
export function waitText(task: WorkTask, tr: Translate): string {
  if (task.cutIn) return tr('Was cut in on; starts again')
  switch (task.wait?.kind) {
    case 'next':
      return tr('Next up')
    case 'behind':
      return counted('{count} ahead', task.wait.ahead, tr)
    case 'after':
      return tr('Follows what it works on')
    case 'usage':
      return tr('Near the usage limit: one at a time')
    default:
      return tr('Waiting')
  }
}

/** How a task stands, beside its title: why it waits, what its thread does, or how it ended (a failed or stopped one says why). */
export function taskLine(task: WorkTask, tr: Translate): string {
  switch (task.status) {
    case 'waiting':
      return waitText(task, tr)
    case 'working':
      return tr(task.activity || 'Working')
    case 'failed':
    case 'stopped':
      return task.result || tr(workStatusWord(task.status))
    default:
      return tr(workStatusWord(task.status))
  }
}

/** Try again is offered on a task that failed or was stopped, unless another bot asked for it: that bot has its
 * answer already. A routine's task runs the routine again. */
export const canRetry = (task: WorkTask) => (task.status === 'failed' || task.status === 'stopped') && task.source?.kind !== 'request'

/** A thread has it and it may give way to a task that cuts in: only work the bot added that holds a seat. A routine's
 * run or another bot's request is stopped, never set aside, and a thread the user started (no seat) never gives way. */
export const givesWay = (task: WorkTask) => isLive(task) && task.source === null && task.seat !== false

/** Every seat is taken, so the task first in line can only start now if a thread gives way, and one may. Not one that
 * follows unfinished work or waits for usage: a freed thread wouldn't start it. */
export function canCutIn(work: WorkTask[], maxThreads: number | null): boolean {
  const first = work.find((t) => t.status === 'waiting')
  const held = first?.wait?.kind === 'after' || first?.wait?.kind === 'usage'
  return !!first && !held && work.some(givesWay) && work.filter(takesSeat).length >= (maxThreads ?? DEFAULT_THREADS)
}

/** A card of a thread's waiting in the chat for the user: an approval, a choice, or a sign-in its agent needs. */
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

/** The card each task that needs the user waits on in the chat, by task id: its thread's newest pending one, or, when
 * that is a sign-in and another (the agent the turn left signed out, while its fallback was too) holds the task's
 * message, that one, whose sign-in sends it again. */
export function pendingCards(entries: Entry[]): Map<string, Entry> {
  const newest = new Map<string, Entry>()
  const holding = new Map<string, Entry>()
  for (const e of entries) {
    const task = e.data.task
    if (!task || !isPendingCard(e)) continue
    newest.set(task, e)
    if (holdsMessages(e)) holding.set(task, e)
  }
  return new Map([...newest].map(([task, e]) => [task, (e.data.connectionRequest && holding.get(task)) || e]))
}

/** What each task that needs the user is asked, by task id: what its card waiting in the chat (`pendingCards`) says
 * (an approval's title, a choice's question, which agent to sign in to). */
export function pendingAsks(entries: Entry[], tr: Translate): Map<string, string> {
  const ask = (e: Entry) => {
    const request = e.data.connectionRequest
    return request ? fill(tr('Needs {agent} sign-in'), { agent: String(request.title ?? '') }) : (e.data.title ?? e.data.question ?? '')
  }
  return new Map([...pendingCards(entries)].map(([task, e]) => [task, ask(e)]))
}

/** Why a thread that takes no message is read only, as a catalog key: its result has an owner already. Null for an
 * task that takes messages, and for the bot's own (nothing to explain). */
export function readOnlyReason(task: Pick<WorkTask, 'talk' | 'source'>): string | null {
  if (task.talk) return null
  switch (task.source?.kind) {
    case 'routine':
      return "Read only: a routine's run"
    case 'request':
      return "Read only: another bot's request"
    default:
      return null
  }
}

/** Where a task's thread works, in one quiet line. */
export interface WhereLine {
  kind: 'folder' | 'branch'
  text: string
}

/** The branch when the task has a worktree of its own, else its folder with the home folder as `~`. Null for a task
 * no thread has taken. */
export function whereLine(task: Pick<WorkTask, 'folder' | 'worktree'>, home: string): WhereLine | null {
  if (task.worktree) return { kind: 'branch', text: task.worktree.branch }
  return task.folder ? { kind: 'folder', text: homeRelative(task.folder, home) } : null
}

/** The full path the where line copies: the thread's folder, or its worktree's from a host that names no folder. */
export const wherePath = (task: Pick<WorkTask, 'folder' | 'worktree'>): string | null => task.folder ?? (task.worktree?.path || null)

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

/** A thread's transcript as its panel shows it: what the host told the thread first (`told`, folded at the top), then
 * the rest in order, without the messages the user took back. What the thread `threadId` put in the bot's `chat` (a
 * file, a voice note, an artifact, a card, answered or not) joins the rest where it came. */
export function threadEntries(entries: Entry[], chat: Entry[] = [], threadId = ''): { told: Entry | null; rest: Entry[] } {
  const shown = entries.filter((e) => !e.data.removed)
  const told = shown.find(isTold) ?? null
  const shared = chat.filter((e) => e.data.thread?.id === threadId && !e.data.removed)
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
