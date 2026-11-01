import assert from 'node:assert/strict'
import { test } from 'node:test'
import { mayAutoClaim, mayJoinAfterInstall, mayOfferMove } from './auto-claim.ts'

const host = { id: 'new', online: true, cloud: { enabled: true, owner: null } }

test('a new identity with the cloud on joins the account on its own', () => {
  assert.equal(mayAutoClaim(host, [], ['old']), true)
})

test('an identity seen in the account and removed since stays out', () => {
  assert.equal(mayAutoClaim(host, [], ['old', 'new']), false)
})

test('no auto-claim when kept out, owned, offline, or the cloud is off or unknown', () => {
  assert.equal(mayAutoClaim(host, ['new'], []), false)
  assert.equal(mayAutoClaim({ ...host, cloud: { enabled: true, owner: { userId: 'u' } } }, [], []), false)
  assert.equal(mayAutoClaim({ ...host, online: false }, [], []), false)
  assert.equal(mayAutoClaim({ ...host, cloud: { enabled: false } }, [], []), false)
  assert.equal(mayAutoClaim({ ...host, cloud: null }, [], []), false)
})

test('a computer on a private relay never joins: it has no accounts', () => {
  assert.equal(mayAutoClaim({ ...host, cloud: { enabled: true, owner: null, relay: 'private' } }, [], []), false)
  assert.equal(mayAutoClaim({ ...host, cloud: { enabled: true, owner: null, relay: 'codync' } }, [], []), true)
})

test('a computer in another account offers to move into the signed-in one', () => {
  const owned = (userId: string) => ({ ...host, cloud: { enabled: true, owner: { userId } } })
  assert.equal(mayOfferMove(owned('a'), 'b', []), true)
  assert.equal(mayOfferMove(owned('b'), 'b', []), false, 'already in this account')
  assert.equal(mayOfferMove(host, 'b', []), false, 'in no account: auto-claim covers it')
  assert.equal(mayOfferMove(owned('a'), 'b', ['new']), false, 'the user said no')
  assert.equal(mayOfferMove({ ...owned('a'), online: false }, 'b', []), false)
  assert.equal(mayOfferMove({ ...host, cloud: { enabled: true, owner: { userId: 'a' }, relay: 'private' } }, 'b', []), false)
})

test('an SSH computer installed from here joins unless it is in an account or on a private relay', () => {
  // Its cloud may still be off: Add to account turns it on, as installing shows the intent.
  assert.equal(mayJoinAfterInstall({ owner: null }), true)
  assert.equal(mayJoinAfterInstall({ owner: null, relay: 'codync' }), true)
  assert.equal(mayJoinAfterInstall({ owner: { userId: 'a' } }), false)
  assert.equal(mayJoinAfterInstall({ owner: null, relay: 'private' }), false)
})
