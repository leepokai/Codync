// Background tasks (host/src/chat/tasks/): long work a bot does away from its chat. The chat shows one capsule
// above the composer; a task's own transcript is read from its details and never enters a chat.

import type { Entry } from './models'

/** A task still going, as its bot carries it (`bot.tasks`). */
export interface BotTask {
  id: string
  title: string
  status: 'starting' | 'working' | 'needsInput'
  /** What it is doing now; empty until it says. */
  activity: string
  startedAt: number
  /** How its session began: a fork of the chat's, or a new one with a brief. */
  start: string
}

/** A task as `taskEntries` answers it: one that ended is still read there. */
export interface TaskRecord {
  id: string
  title: string
  status: 'starting' | 'working' | 'done' | 'failed' | 'stopped'
  startedAt: number
  endedAt?: number | null
  start: string
}

type Translate = (text: string) => string

/** `bot.tasks`; a host from before tasks sends none, and the feature doesn't show. */
export function normalizeTasks(raw: unknown): BotTask[] {
  if (!Array.isArray(raw)) return []
  return (raw as Partial<BotTask>[]).flatMap((t) =>
    t && typeof t.id === 'string'
      ? [{
          id: t.id,
          title: t.title ?? '',
          status: t.status === 'starting' || t.status === 'needsInput' ? t.status : 'working',
          activity: t.activity ?? '',
          startedAt: t.startedAt ?? 0,
          start: t.start ?? 'brief',
        }]
      : [],
  )
}

/** A `taskEntry` event: one entry of a task's transcript. Null for one this app can't read. */
export function parseTaskEntry(raw: Record<string, unknown>): { task: string; entry: Entry } | null {
  const entry = raw.entry as Entry | undefined
  if (typeof raw.task !== 'string' || !entry || typeof entry.id !== 'string') return null
  return { task: raw.task, entry: { ...entry, data: entry.data ?? {} } }
}

/** What one task is doing, in a few words. */
export function taskWhat(task: BotTask, tr: Translate): string {
  if (task.status === 'needsInput') return tr('Needs you')
  if (task.status === 'starting') return tr('Starting…')
  return task.activity ? tr(task.activity) : task.title
}

/** What the tasks are doing: the one task's, or how many there are (and whether any waits for the user). */
export function tasksWhat(tasks: BotTask[], tr: Translate): string {
  if (tasks.length === 1) return taskWhat(tasks[0]!, tr)
  const count = tr('{count} tasks running').replaceAll('{count}', () => String(tasks.length))
  return tasks.some((t) => t.status === 'needsInput') ? `${count} · ${tr('Needs you').toLowerCase()}` : count
}

/** When the oldest of the tasks started: the capsule's time. */
export const tasksSince = (tasks: BotTask[]) => Math.min(...tasks.map((t) => t.startedAt))

/** A roster row's preview while the chat itself is idle and tasks go on; null when the row reads as usual. */
export function rosterTaskLine(bot: { status: string; tasks: BotTask[] }, tr: Translate): string | null {
  if (bot.status !== 'idle' || bot.tasks.length === 0) return null
  return tr('In the background: {what}').replaceAll('{what}', () => tasksWhat(bot.tasks, tr))
}

/** A task's status in words: the live one's from its bot, an ended one's from its record. */
export function taskStatusWord(status: BotTask['status'] | TaskRecord['status']): string {
  switch (status) {
    case 'starting':
      return 'Starting…'
    case 'working':
      return 'Working'
    case 'needsInput':
      return 'Needs you'
    case 'done':
      return 'Done'
    case 'failed':
      return 'Failed'
    case 'stopped':
      return 'Stopped'
  }
}

/** A task's transcript with more of its entries, oldest first: each entry once, the higher `rev` kept. */
export function mergeTaskEntries(held: Entry[], incoming: Entry[]): Entry[] {
  const byId = new Map(held.map((e) => [e.id, e]))
  for (const e of incoming) {
    const have = byId.get(e.id)
    if (!have || e.rev >= have.rev) byId.set(e.id, e)
  }
  return [...byId.values()].sort((a, b) => a.seq - b.seq)
}

/** What the task was told (its brief, the bot's `tell_task`): the host wrote it, the user didn't type it. */
export const isTold = (e: Entry) => e.kind === 'user' && e.data.from === 'host'
