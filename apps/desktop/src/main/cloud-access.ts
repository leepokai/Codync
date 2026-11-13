import { randomBytes } from 'node:crypto'
import { setTimeout as sleep } from 'node:timers/promises'
import { ipcMain } from 'electron'
import type { AccessComputer, AccessResult } from '../shared/ipc'
import type { AccountService } from './account'
import { b64url, sasCode, sasCommit, unb64url } from './channel/crypto.ts'
import { cloudRequest, identity } from './cloud'

// Asking an account computer for access (docs/reference/remote-relay.md §4.2 B): commit to a nonce, reveal it
// once the computer answered with its own, then wait for its decision. `onCode` gets the 6-digit code both sides
// then show, for the person to compare; a Codync Cloud computer approves this account's devices by itself.
// The nonce never leaves this function before the reveal.

const POLL_MS = 1500

type Status = { status: string; hostNonce?: string }

async function requestAccess(account: AccountService, computer: AccessComputer, onCode: (code: string) => void, signal: AbortSignal): Promise<AccessResult> {
  const user = account.state.user
  if (!user) return { ok: false, error: { status: 0, code: 'signedOut', message: 'Sign in again to reach your account.' } }
  const deviceKey = unb64url((await identity(user.userId)).publicKey)
  const nD = randomBytes(32)
  const created = await cloudRequest(account, 'POST', `v1/computers/${computer.computerId}/access-requests`, { commit: b64url(sasCommit(deviceKey, nD)) }, true)
  if (!created.ok) return created
  const requestId = String(created.value.requestId)
  const expiresAt = Number(created.value.expiresAt)
  /** The request's status once `done` accepts it, or a settled one; null once it expired here or was withdrawn. */
  const wait = async (done: (s: Status) => boolean): Promise<{ ok: true; value: Status } | Extract<AccessResult, { ok: false }> | null> => {
    while (Date.now() < expiresAt && !signal.aborted) {
      const res = await cloudRequest(account, 'GET', `v1/access-requests/${requestId}`, undefined, false)
      if (!res.ok) return res
      const s = res.value as unknown as Status
      if (s.status !== 'pending' || done(s)) return { ok: true, value: s }
      await sleep(POLL_MS, undefined, { signal }).catch(() => {})
    }
    return null
  }
  const answered = await wait((s) => !!s.hostNonce)
  if (answered?.ok && answered.value.status === 'pending') {
    const revealed = await cloudRequest(account, 'POST', `v1/access-requests/${requestId}/reveal`, { nonce: b64url(nD) }, true)
    if (!revealed.ok) return revealed
    onCode(sasCode(unb64url(computer.signKey), deviceKey, nD, unb64url(answered.value.hostNonce!)))
    const decided = await wait(() => false)
    if (decided) return decided.ok ? { ok: true, value: { status: decided.value.status } } : decided
  } else if (answered) {
    return answered.ok ? { ok: true, value: { status: answered.value.status } } : answered
  }
  // Withdrawn here, or nobody answered in time: take the request back, so the computer's list doesn't keep it.
  await cloudRequest(account, 'DELETE', `v1/access-requests/${requestId}`, undefined, false)
  return { ok: true, value: { status: signal.aborted ? 'cancelled' : 'expired' } }
}

export function registerCloudAccess(account: AccountService) {
  /** The request still waiting here for each computer, and the promise of the last one to settle. */
  const waiting = new Map<string, { abort: AbortController; settled: Promise<void> }>()
  ipcMain.handle('cloud:requestAccess', async (e, computer: AccessComputer, requestId: string) => {
    const id = computer.computerId
    const previous = waiting.get(id)
    previous?.abort.abort()
    const abort = new AbortController()
    const onCode = (code: string) => {
      if (!e.sender.isDestroyed()) e.sender.send('cloud:accessCode', requestId, code)
    }
    const run = (async () => {
      // The computer cancels its pending request when a new one arrives, so the previous one must be gone first.
      await previous?.settled
      try {
        if (abort.signal.aborted) return { ok: true, value: { status: 'cancelled' } }
        return await requestAccess(account, computer, onCode, abort.signal)
      } finally {
        if (waiting.get(id)?.abort === abort) waiting.delete(id)
      }
    })()
    waiting.set(id, { abort, settled: run.then(() => {}, () => {}) })
    return run
  })
  ipcMain.on('cloud:cancelAccess', (_e, computerId: string) => waiting.get(computerId)?.abort.abort())
}
