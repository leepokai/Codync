import { randomBytes } from 'node:crypto'
import { ipcMain } from 'electron'
import type { AccountService } from './account'
import { b64url, sasCommit, unb64url } from './channel/crypto.ts'
import { cloudRequest, identity, type CloudFailure } from './cloud'

// Asking an account computer for access (docs/reference/remote-relay.md §4.2 B): commit to a nonce, reveal it
// once the computer answered with its own. Only for Codync Cloud computers, which approve this account's
// devices by themselves, so nobody compares the code. The nonce never leaves this function before the reveal.

const POLL_MS = 1500

type Status = { status: string; hostNonce?: string; expiresAt?: number }

async function requestAccess(account: AccountService, computerId: string): Promise<{ ok: true; value: { status: string } } | { ok: false; error: CloudFailure }> {
  const user = account.state.user
  if (!user) return { ok: false, error: { status: 0, code: 'signedOut', message: 'Sign in again to reach your account.' } }
  const nD = randomBytes(32)
  const commit = b64url(sasCommit(unb64url((await identity(user.userId)).publicKey), nD))
  const created = await cloudRequest(account, 'POST', `v1/computers/${computerId}/access-requests`, { commit }, true)
  if (!created.ok) return created
  const requestId = String(created.value.requestId)
  const expiresAt = Number(created.value.expiresAt)
  /** The request's status once `done` accepts it, or a settled one; null once it expired here. */
  const wait = async (done: (s: Status) => boolean): Promise<{ ok: true; value: Status } | { ok: false; error: CloudFailure } | null> => {
    while (Date.now() < expiresAt) {
      const res = await cloudRequest(account, 'GET', `v1/access-requests/${requestId}`, undefined, false)
      if (!res.ok) return res
      const s = res.value as unknown as Status
      if (s.status !== 'pending' || done(s)) return { ok: true, value: s }
      await new Promise((r) => setTimeout(r, POLL_MS))
    }
    return null
  }
  const answered = await wait((s) => !!s.hostNonce)
  if (answered?.ok && answered.value.status === 'pending') {
    const revealed = await cloudRequest(account, 'POST', `v1/access-requests/${requestId}/reveal`, { nonce: b64url(nD) }, true)
    if (!revealed.ok) return revealed
    const decided = await wait(() => false)
    if (decided) return decided.ok ? { ok: true, value: { status: decided.value.status } } : decided
  } else if (answered) {
    return answered.ok ? { ok: true, value: { status: answered.value.status } } : answered
  }
  // Nobody answered in time: take the request back, so the computer's list doesn't keep it.
  await cloudRequest(account, 'DELETE', `v1/access-requests/${requestId}`, undefined, false)
  return { ok: true, value: { status: 'expired' } }
}

export function registerCloudAccess(account: AccountService) {
  ipcMain.handle('cloud:requestAccess', (_e, computerId: string) => requestAccess(account, computerId))
}
