import { isBelow } from '../../../shared/compat.ts'
import type { Entry } from '../../../shared/models.ts'

/** The first host that steers a queued message (`steer`) and takes one back (`unqueue`). */
const STEER_HOST = '2.39.0'

/** Whether the computer's host can steer and take back queued messages; an older one only queues them. */
export function canSteer(hostVersion: string | undefined): boolean {
  return !!hostVersion && !isBelow(hostVersion, STEER_HOST)
}

/** Whether the computer's host holds messages for a later time (`scheduleSend`). */
export function canSchedule(hello: { scheduleSend?: boolean } | undefined | null): boolean {
  return hello?.scheduleSend === true
}

/** One of the user's messages queued behind the bot's work. */
const waits = (e: Entry) => e.kind === 'user' && e.data.status === 'queued'

/** One of the user's messages held for a later time (`scheduleSend`): the tray shows it until it fires. */
const scheduled = (e: Entry) => e.kind === 'user' && e.data.status === 'scheduled'

/** No id the chat has taken back: what a caller that knows nothing about removals passes. */
const NONE: ReadonlySet<string> = new Set()

/**
 * The user's pending messages for the tray: the live queue (while the bot works, oldest first), then the
 * scheduled ones by fire time — one tray, whether the bot is busy or not.
 */
export function waitingMessages(entries: Entry[], botWorking: boolean): Entry[] {
  const live = botWorking ? entries.filter(waits) : []
  const held = entries.filter(scheduled).sort((a, b) => (a.data.notBefore ?? 0) - (b.data.notBefore ?? 0))
  return [...live, ...held]
}

/**
 * The held messages the chat's own entries do not carry: the host's `hello.held` lists every message still
 * scheduled in any chat, so one older than the chat's loaded window still reaches the tray. An entry the chat
 * holds wins — an edit, a cancel or the fire itself lands there first. `takenBack` holds the ids the chat has
 * dropped by taking them back (`removed`): `hello.held` is a snapshot, so its copy of one is stale, and the
 * chat no longer shows that row to correct it.
 */
export function heldOutside(entries: Entry[], botId: string, held: Entry[] | undefined, takenBack: ReadonlySet<string> = NONE): Entry[] {
  if (!held?.length) return []
  return held.filter(
    (e) => e.botId === botId && scheduled(e) && !takenBack.has(e.id) && !entries.some((x) => x.id === e.id),
  )
}

/** The chat's entries without the ones the queue shows. */
export function outsideQueue(entries: Entry[], botWorking: boolean): Entry[] {
  return entries.filter((e) => !(botWorking && waits(e)) && !scheduled(e))
}
