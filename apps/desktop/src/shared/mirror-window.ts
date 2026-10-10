import type { Entry } from './models.ts'

/** A catch-up can be filled by thread replies, leaving fewer main-chat entries than a page. */
export function canLoadHistory(entries: Entry[], complete: boolean): boolean {
  return !complete && entries.some((e) => e.seq > 0 && e.seq !== Number.MAX_SAFE_INTEGER)
}

/**
 * Lowest `seq` among a bot's main-chat entries the mirror had at `since` (optimistic `local-*` ones
 * are left out), or undefined. Newer entries (a send's RPC response landing before this hello) come
 * again in this catch-up and say nothing about the window.
 */
export function mainChatFloor(entries: Entry[], since: number): number | undefined {
  let floor: number | undefined
  for (const e of entries) {
    if (e.threadId || e.seq <= 0 || e.id.startsWith('local-') || e.rev > since) continue
    floor = floor === undefined ? e.seq : Math.min(floor, e.seq)
  }
  return floor
}

/** The per-bot floors for a connection that resumes from `since`: none when it starts from scratch. */
export function snapshotFloors(entries: Map<string, Entry[]>, since: number): Map<string, number> {
  const floors = new Map<string, number>()
  if (since <= 0) return floors
  for (const [botId, list] of entries) {
    const floor = mainChatFloor(list, since)
    if (floor !== undefined) floors.set(botId, floor)
  }
  return floors
}

/**
 * Whether an entry event is a main-chat entry the mirror does not hold that sits below the bot's
 * floor. `seq` follows insert order, so everything created after a connection's `since` is above
 * the floor; an unknown entry below it is an old one the host re-stamped (a rewrite, a reaction)
 * and arrives with history paging. Inserting it would make `loadOlder` skip the gap.
 */
export function isOutsideLoadedWindow(floor: number | undefined, held: boolean, e: Entry): boolean {
  if (floor === undefined || held || e.threadId) return false
  return e.seq < floor
}
