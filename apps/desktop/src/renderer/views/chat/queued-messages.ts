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

/**
 * The user's pending messages for the tray: the live queue (while the bot works, oldest first), then the
 * scheduled ones by fire time — one tray, whether the bot is busy or not.
 */
export function waitingMessages(entries: Entry[], botWorking: boolean): Entry[] {
  const live = botWorking ? entries.filter(waits) : []
  const held = entries.filter(scheduled).sort((a, b) => (a.data.notBefore ?? 0) - (b.data.notBefore ?? 0))
  return [...live, ...held]
}

/** The chat's entries without the ones the queue shows. */
export function outsideQueue(entries: Entry[], botWorking: boolean): Entry[] {
  return entries.filter((e) => !(botWorking && waits(e)) && !scheduled(e))
}
