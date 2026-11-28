// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
//
// Such a host says `item` for a task and `task` for a thread. `HostClient` passes what it sends and receives through
// here, so the stores, models and views know only the names of today. What comes in is read by its shape, in either
// form; what goes out names a task and a bot's thread settings both ways, and only a thread's transcript is asked
// for by the host's version (`speaksThreads`).

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

/** `fields` with what `name` holds also under `old`: each host reads the name it knows and ignores the other. */
function also(fields: Fields, name: string, old: string): Fields {
  return name in fields ? { ...fields, [old]: fields[name] } : fields
}

/** The methods that name a task. */
const NAMES_TASK = new Set(['send', 'steer', 'unqueue', 'workFirst', 'workCutIn', 'dropWork', 'stopWork', 'retryWork'])

/**
 * A call as either host takes it: its method and body. What names a task or a bot's thread settings carries both
 * names whatever `hostVersion` says, so a version that is missing or left from the last connection never sends a
 * thread's message to the chat. Only `threadEntries` goes by the version: a wrong guess there is a read that fails.
 */
export function toHost(hostVersion: string | undefined, method: string, body: unknown): [string, unknown] {
  if (!isFields(body)) return [method, body]
  if (method === 'threadEntries') return speaksThreads(hostVersion) ? [method, body] : ['taskEntries', renamed(body, 'threadId', 'taskId')]
  if (NAMES_TASK.has(method)) return [method, also(body, 'taskId', 'itemId')]
  if (method === 'updateBot' || method === 'createBot') return [method, also(also(body, 'maxThreads', 'maxAgents'), 'threadWorktrees', 'taskWorktrees')]
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

/**
 * What an `updateBot` answer says about a pause or resume the client just asked for.
 *
 * A current host echoes `bot.paused` as a boolean: it always serializes the flag. An older host
 * ignores the unknown field and still returns success, with `paused` absent. A missing flag is not
 * an applied pause — the client must not show Paused, or count it as applied, on that success.
 */
export type PauseEcho = { applied: boolean; paused: boolean }

export function pauseEcho(requested: boolean, answer: unknown): PauseEcho {
  const echoed = isFields(answer) && isFields(answer.bot) ? answer.bot.paused : undefined
  if (typeof echoed !== 'boolean') return { applied: false, paused: false }
  return { applied: echoed === requested, paused: echoed }
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
