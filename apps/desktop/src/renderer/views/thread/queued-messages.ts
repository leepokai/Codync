import { isBelow } from '../../../shared/compat.ts'
import type { Entry } from '../../../shared/models.ts'

/** The first host that steers a queued message (`steer`) and takes one back (`unqueue`). */
const STEER_HOST = '2.39.0'

/** Whether the computer's host can steer and take back queued messages; an older one only queues them. */
export function canSteer(hostVersion: string | undefined): boolean {
  return !!hostVersion && !isBelow(hostVersion, STEER_HOST)
}

/** One of the user's messages queued behind the bot's work. */
const waits = (e: Entry) => e.kind === 'user' && e.data.status === 'queued'

/**
 * The user's messages waiting while the bot works, oldest first: the queue above the composer shows them instead of
 * the chat. An idle bot takes its next message at once, so the chat keeps showing that one (no flicker).
 */
export function waitingMessages(entries: Entry[], botWorking: boolean): Entry[] {
  return botWorking ? entries.filter(waits) : []
}

/** The chat's entries without the ones the queue shows. */
export function outsideQueue(entries: Entry[], botWorking: boolean): Entry[] {
  return botWorking ? entries.filter((e) => !waits(e)) : entries
}
