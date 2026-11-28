import { normalizeWork, type WorkTask } from '@shared/work'

/** An ended thread of a bot's from before the last day, which its `work` no longer carries (`olderThreads`). */
export type OlderThread = WorkTask & { botId: string }

/** One page of `olderThreads`: ended threads of every bot on the computer, newest first. */
export function parseOlderThreads(raw: unknown): { threads: OlderThread[]; more: boolean } {
  const page = raw as { threads?: unknown; more?: unknown } | null
  const threads = Array.isArray(page?.threads) ? (page.threads as { botId?: unknown }[]) : []
  return {
    threads: threads.flatMap((t) => (typeof t?.botId === 'string' ? normalizeWork([t]).map((task) => ({ ...task, botId: t.botId as string })) : [])),
    more: page?.more === true,
  }
}

/** `roster` with each bot's older threads after its own `work`; a task its `work` has keeps that row. */
export function withOlderThreads<T extends { bot: { id: string; work: WorkTask[] } }>(roster: T[], olderOf: (of: T) => OlderThread[]): T[] {
  return roster.map((of) => {
    const older = olderOf(of).filter((t) => t.botId === of.bot.id && !of.bot.work.some((w) => w.id === t.id))
    return older.length ? { ...of, bot: { ...of.bot, work: [...of.bot.work, ...older] } } : of
  })
}

/** What "Delete old threads" offers: never, a week, a month, three months. */
const KEEP_CHOICES = [0, 7, 30, 90]

/** The choices in order, with the computer's own number where another device set one the menu lacks. */
export function keepChoices(current: number): number[] {
  return [...new Set([...KEEP_CHOICES, current])].sort((a, b) => a - b)
}

const key = (t: { botId: string; id: string }) => `${t.botId}/${t.id}`

/** A computer's older threads, a page at a time: the first once the Threads list shows, the next as it scrolls to its end. */
export class OlderThreads {
  /** The pages the host sent; the next asks before the oldest of them. */
  rows: OlderThread[] = []
  /** Ended threads that left their bot's `work` (past its last day) while connected, as they last read. */
  private kept = new Map<string, OlderThread>()
  /** Threads dropped here: their bot's next `work` without them doesn't keep them. */
  private dropped = new Set<string>()
  /** The host has more before the oldest row. */
  more = true
  /** A page was asked for since the last `reset`. */
  started = false
  private loading = false
  private epoch = 0

  private readonly changed: () => void

  constructor(changed: () => void) {
    this.changed = changed
  }

  /** A new hello (a reconnect, another host): what was listed may be stale. */
  reset() {
    this.epoch++
    this.rows = []
    this.kept.clear()
    this.dropped.clear()
    this.more = true
    this.started = false
    this.loading = false
  }

  /** Every older thread to list: the ones kept from `work` over their paged rows, which may be stale. */
  get shown(): OlderThread[] {
    return [...this.kept.values(), ...this.rows.filter((t) => !this.kept.has(key(t)))]
  }

  /** A bot's `work` changed from `was` to `now`: an ended thread it no longer carries stays listed, as it last read. */
  keep(botId: string, was: WorkTask[] | undefined, now: WorkTask[]) {
    const left = (was ?? []).filter((t) => t.endedAt !== null && !now.some((w) => w.id === t.id) && !this.dropped.has(key({ botId, id: t.id })))
    for (const t of left) this.kept.set(key({ botId, id: t.id }), { ...t, botId })
    if (left.length) this.changed()
  }

  /** Takes a task out (dropped); the returned function puts it back. */
  drop(botId: string, taskId: string): () => void {
    const [rows, kept] = [this.rows, new Map(this.kept)]
    this.rows = rows.filter((t) => t.botId !== botId || t.id !== taskId)
    const was = this.kept.delete(key({ botId, id: taskId }))
    this.dropped.add(key({ botId, id: taskId }))
    if (this.rows.length !== rows.length || was) this.changed()
    return () => {
      this.rows = rows
      this.kept = kept
      this.dropped.delete(key({ botId, id: taskId }))
      this.changed()
    }
  }

  /** The next page, asked with `before` the oldest row's `endedAt` (none for the first). */
  async loadNext(ask: (before: number | undefined) => Promise<unknown>) {
    if (this.loading || !this.more) return
    this.loading = true
    this.started = true
    const epoch = this.epoch
    const ended = this.rows.flatMap((t) => (t.endedAt === null ? [] : [t.endedAt]))
    try {
      const page = parseOlderThreads(await ask(ended.length ? Math.min(...ended) : undefined))
      if (epoch !== this.epoch) return
      const have = new Set(this.rows.map((t) => `${t.botId}/${t.id}`))
      const added = page.threads.filter((t) => !have.has(`${t.botId}/${t.id}`))
      this.rows = [...this.rows, ...added]
      // A page that adds nothing would ask for itself again: stop there.
      this.more = page.more && added.length > 0
      this.changed()
    } catch (e) {
      // ponytail: a failed page stops paging until the next reset (a reconnect), so a list end in view can't loop on it.
      if (epoch === this.epoch) this.more = false
      throw e
    } finally {
      if (epoch === this.epoch) this.loading = false
    }
  }
}
