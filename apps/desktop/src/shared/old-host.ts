// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
//
// Such a host says `item` for a task and `task` for a thread. `HostClient` passes what it sends and receives through
// here, so the stores, models and views know only the names of today. What comes in is read by its shape, in either
// form; what goes out is renamed only for a host that `speaksThreads` says is older.

import { isBelow } from './compat.ts'
import type { Entry } from './models'

/** The first host that says task and thread. */
export const THREADS_HOST = '2.51.0'

/** Whether the computer's host takes the names of today; so does one whose version isn't known yet, as on the phones. */
export function speaksThreads(hostVersion: string | undefined): boolean {
  return !hostVersion || !isBelow(hostVersion, THREADS_HOST)
}

type Fields = Record<string, unknown>

const isFields = (value: unknown): value is Fields => typeof value === 'object' && value !== null && !Array.isArray(value)

/** `fields` with `from` named `to`; unchanged without `from`. */
function renamed(fields: Fields, from: string, to: string): Fields {
  if (!(from in fields)) return fields
  const { [from]: value, ...rest } = fields
  return { ...rest, [to]: value }
}

/** The methods that name a task. */
const NAMES_TASK = new Set(['send', 'steer', 'unqueue', 'workFirst', 'workCutIn', 'dropWork', 'stopWork', 'retryWork'])

/** A call as an older host takes it: its method and body. */
export function toOldHost(method: string, body: unknown): [string, unknown] {
  if (!isFields(body)) return [method, body]
  if (method === 'threadEntries') return ['taskEntries', renamed(body, 'threadId', 'taskId')]
  if (NAMES_TASK.has(method)) return [method, renamed(body, 'taskId', 'itemId')]
  if (method === 'updateBot' || method === 'createBot') return [method, renamed(renamed(body, 'maxThreads', 'maxAgents'), 'threadWorktrees', 'taskWorktrees')]
  return [method, body]
}

/** A bot as either host sends it, in the names of today. */
export function fromOldBot(raw: unknown): unknown {
  if (!isFields(raw)) return raw
  const bot = renamed(renamed(raw, 'maxAgents', 'maxThreads'), 'taskWorktrees', 'threadWorktrees')
  if (!Array.isArray(bot.work)) return bot
  return { ...bot, work: bot.work.map((task: unknown) => (isFields(task) && !('thread' in task) ? renamed(task, 'task', 'thread') : task)) }
}

/** An entry as either host sends it: an older one's `data.task` is the thread (`{id, title}`) and `data.item` the task's id. */
export function fromOldEntry<T>(entry: T): T {
  const data = isFields(entry) ? entry.data : null
  if (!isFields(data)) return entry
  const thread = isFields(data.task)
  if (!thread && !('item' in data)) return entry
  const { item, task, ...rest } = data
  return { ...entry, data: { ...rest, ...(thread ? { thread: task } : {}), task: item ?? null } }
}

/** An event as either host sends it: an older one's `taskEntry {task, entry}` is `threadEntry {thread, entry}`. */
export function fromOldEvent(raw: Fields): Fields {
  if (raw.type === 'taskEntry') return { ...renamed(raw, 'task', 'thread'), type: 'threadEntry' }
  if (raw.type === 'bot') return { ...raw, bot: fromOldBot(raw.bot) }
  if (raw.type === 'entry') return { ...raw, entry: fromOldEntry(raw.entry) }
  return raw
}

/** The answer to `method` as either host sends it: its `bot`, `entry` and `entries`, and `threadEntries`' `thread`. */
export function fromOldAnswer(method: string, answer: unknown): unknown {
  if (!isFields(answer)) return answer
  const out = method === 'threadEntries' && !('thread' in answer) ? renamed(answer, 'task', 'thread') : { ...answer }
  if ('bot' in out) out.bot = fromOldBot(out.bot)
  if ('entry' in out) out.entry = fromOldEntry(out.entry)
  if (Array.isArray(out.entries)) out.entries = (out.entries as Entry[]).map(fromOldEntry)
  return out
}
