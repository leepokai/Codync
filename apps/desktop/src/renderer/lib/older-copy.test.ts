import assert from 'node:assert/strict'
import { test } from 'node:test'
import { isOlderCopy } from './older-copy.ts'

test('an offline same-named computer is an older copy only if last seen before the current one joined', () => {
  const old = { computerId: 'old', name: 'Mac', online: false, lastSeenAt: 200 }
  const current = (claimedAt: number) => ({ computerId: 'new', name: 'Mac', online: true, claimedAt })
  assert.equal(isOlderCopy(old, [old, current(100)], []), false)
  assert.equal(isOlderCopy(old, [old, current(300)], []), true)
  // Used here without being listed in the account.
  assert.equal(isOlderCopy(old, [old], [{ id: 'new', name: 'Mac' }]), true)
  assert.equal(isOlderCopy({ ...old, online: true }, [old, current(300)], []), false)
})
