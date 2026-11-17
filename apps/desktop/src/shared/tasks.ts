// A task (host/src/chat/tasks/): the agent session that works an item of a bot's box (`work.ts`). Its own
// transcript is read from the item's details and never enters a chat.

import type { Entry } from './models'

/** A `taskEntry` event: one entry of a task's transcript. Null for one this app can't read. */
export function parseTaskEntry(raw: Record<string, unknown>): { task: string; entry: Entry } | null {
  const entry = raw.entry as Entry | undefined
  if (typeof raw.task !== 'string' || !entry || typeof entry.id !== 'string') return null
  return { task: raw.task, entry: { ...entry, data: entry.data ?? {} } }
}

/** The id of a message sent to a thread from here, shown until the host's own entry (same `clientNonce`) replaces it. */
export const localEntryId = (nonce: string) => `local-${nonce}`

/** A thread's messages sent from here that the host hasn't taken yet (on their way, or failed and waiting for Resend or
 * Delete): what its transcript keeps while the thread is closed. */
export const unsentEntries = (entries: Entry[]) => entries.filter((e) => e.data.clientNonce && e.id === localEntryId(e.data.clientNonce) && !e.data.removed)

/** A task's transcript with more of its entries, oldest first: each entry once, the higher `rev` kept, and a message
 * sent from here gone once the host's entry for it is in. */
export function mergeTaskEntries(held: Entry[], incoming: Entry[]): Entry[] {
  const arrived = new Set(incoming.flatMap((e) => (e.data.clientNonce && e.id !== localEntryId(e.data.clientNonce) ? [localEntryId(e.data.clientNonce)] : [])))
  const byId = new Map(held.filter((e) => !arrived.has(e.id)).map((e) => [e.id, e]))
  for (const e of incoming) {
    const have = byId.get(e.id)
    if (!have || e.rev >= have.rev) byId.set(e.id, e)
  }
  return [...byId.values()].sort((a, b) => a.seq - b.seq)
}

/** What the task was told (its item: the title, the brief, the user's words): the host wrote it, the user didn't type it. */
export const isTold = (e: Entry) => e.kind === 'user' && e.data.from === 'host'
