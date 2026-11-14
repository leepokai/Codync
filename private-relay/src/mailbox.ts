/** Offline mailbox (§7.7). The blob stays ciphertext. One item is in flight per computer. */

import { b64url, fromB64url } from './codec.ts'
import type { RelayStore } from './store.ts'

const MBOX_MAX_BLOB = 64 * 1024
const MBOX_PER_DEVICE = 50
const MBOX_PER_COMPUTER = 500
const MBOX_MAX_BYTES = 8 * 1024 * 1024
const MBOX_TTL_MS = 24 * 3600_000
const MBOX_MIN_BLOB = 32 + 64 + 16
const FAIL_CODES = new Set(['unauthorized', 'unknownBot', 'invalid'])

export interface MailboxNote {
  dk: string
  text: string
}

type Msg = Record<string, unknown>

const err = (nonce: unknown, code: string): string => JSON.stringify({ t: 'mbox.err', nonce: typeof nonce === 'string' ? nonce : null, code })

export function putMailbox(store: RelayStore, computerId: string, dk: string, msg: Msg, hostReady: boolean, now: number): string {
  const nonce = msg.nonce
  if (typeof nonce !== 'string' || !nonce || nonce.length > 128) return err(nonce, 'invalid')
  const existing = store.mailboxByNonce(computerId, dk, nonce)
  if (existing) return JSON.stringify({ t: 'mbox.ok', nonce, state: existing.state })
  if (hostReady) return err(nonce, 'hostOnline')
  if (typeof msg.d === 'string' && msg.d.length > Math.ceil((MBOX_MAX_BLOB * 4) / 3) + 8) return err(nonce, 'tooLarge')
  const blob = fromB64url(msg.d)
  if (!blob || blob.length < MBOX_MIN_BLOB) return err(nonce, 'invalid')
  if (blob.length > MBOX_MAX_BLOB) return err(nonce, 'tooLarge')
  const exp = msg.exp === undefined ? now + MBOX_TTL_MS : msg.exp
  if (typeof exp !== 'number' || !Number.isSafeInteger(exp) || exp <= now || exp > now + MBOX_TTL_MS) return err(nonce, 'invalid')
  const counts = store.mailboxCounts(computerId, dk)
  if (counts.mine >= MBOX_PER_DEVICE || counts.all >= MBOX_PER_COMPUTER || counts.bytes + blob.length > MBOX_MAX_BYTES) return err(nonce, 'full')
  store.insertMailbox(computerId, dk, nonce, blob, exp)
  return JSON.stringify({ t: 'mbox.ok', nonce, state: 'queued' })
}

export function cancelMailbox(store: RelayStore, computerId: string, dk: string, msg: Msg): string {
  const nonce = typeof msg.nonce === 'string' ? msg.nonce : ''
  if (store.cancelQueued(computerId, dk, nonce)) return JSON.stringify({ t: 'mbox.cancelled', nonce })
  const state = store.mailboxByNonce(computerId, dk, nonce)?.state
  return JSON.stringify({ t: 'mbox.gone', nonce, state: state === 'delivering' ? 'delivering' : 'unknown' })
}

export function listMailbox(store: RelayStore, computerId: string, dk: string): string {
  return JSON.stringify({ t: 'mbox.items', items: store.listMailbox(computerId, dk) })
}

/** The next queued item, marked delivering. `null` when one is already in flight or the queue is empty. */
export function deliverNext(store: RelayStore, computerId: string, now: number): string | null {
  if (store.delivering(computerId)) return null
  const row = store.nextQueued(computerId, now)
  if (!row) return null
  store.markDelivering(row.seq)
  return JSON.stringify({
    t: 'mbox.item',
    seq: row.seq,
    from: row.dk,
    nonce: row.nonce,
    d: b64url(row.blob),
    exp: row.exp,
  })
}

/** Applies the host's ack and returns the note for that device, if the seq was in flight. */
export function ackMailbox(store: RelayStore, computerId: string, msg: Msg): MailboxNote | null {
  const seq = typeof msg.seq === 'number' ? msg.seq : Number(msg.seq)
  if (!Number.isSafeInteger(seq)) return null
  const row = store.takeDelivering(computerId, seq)
  if (!row) return null
  const note =
    msg.ok === true
      ? { t: 'mbox.delivered', nonce: row.nonce }
      : { t: 'mbox.failed', nonce: row.nonce, code: FAIL_CODES.has(msg.code as string) ? msg.code : 'invalid' }
  return { dk: row.dk, text: JSON.stringify(note) }
}

export function expireMailbox(store: RelayStore, computerId: string, now: number): MailboxNote[] {
  return store.takeExpired(computerId, now).map((row) => ({ dk: row.dk, text: JSON.stringify({ t: 'mbox.expired', nonce: row.nonce }) }))
}
