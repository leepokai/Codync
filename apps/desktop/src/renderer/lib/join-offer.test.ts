import assert from 'node:assert/strict'
import { test } from 'node:test'
import { accountMismatch, mayOfferJoin, mayJoinAfterInstall, mayOfferMove } from './join-offer.ts'

const host = { id: 'new', online: true, cloud: { enabled: true, owner: null } }

test('a new identity with the cloud on is asked about', () => {
  assert.equal(mayOfferJoin(host, [], ['old']), true)
})

test('an identity seen in the account and removed since stays out', () => {
  assert.equal(mayOfferJoin(host, [], ['old', 'new']), false)
})

test('nothing is asked when kept out, owned, offline, or the cloud is off or unknown', () => {
  assert.equal(mayOfferJoin(host, ['new'], []), false)
  assert.equal(mayOfferJoin({ ...host, cloud: { enabled: true, owner: { userId: 'u' } } }, [], []), false)
  assert.equal(mayOfferJoin({ ...host, online: false }, [], []), false)
  assert.equal(mayOfferJoin({ ...host, cloud: { enabled: false } }, [], []), false)
  assert.equal(mayOfferJoin({ ...host, cloud: null }, [], []), false)
})

test('a computer on a private relay is never asked about: it has no accounts', () => {
  assert.equal(mayOfferJoin({ ...host, cloud: { enabled: true, owner: null, relay: 'private' } }, [], []), false)
  assert.equal(mayOfferJoin({ ...host, cloud: { enabled: true, owner: null, relay: 'codync' } }, [], []), true)
})

test('a computer in another account offers to move into the signed-in one', () => {
  const owned = (userId: string) => ({ ...host, cloud: { enabled: true, owner: { userId } } })
  assert.equal(mayOfferMove(owned('a'), 'b', []), true)
  assert.equal(mayOfferMove(owned('b'), 'b', []), false, 'already in this account')
  assert.equal(mayOfferMove(host, 'b', []), false, 'in no account: the join offer covers it')
  assert.equal(mayOfferMove(owned('a'), 'b', ['new']), false, 'the user said no')
  assert.equal(mayOfferMove({ ...owned('a'), online: false }, 'b', []), false)
  assert.equal(mayOfferMove({ ...host, cloud: { enabled: true, owner: { userId: 'a' }, relay: 'private' } }, 'b', []), false)
})

test('an SSH computer installed from here is asked about unless it is in an account or on a private relay', () => {
  // Its cloud may still be off: Add to account turns it on, as installing shows the intent.
  assert.equal(mayJoinAfterInstall({ owner: null }), true)
  assert.equal(mayJoinAfterInstall({ owner: null, relay: 'codync' }), true)
  assert.equal(mayJoinAfterInstall({ owner: { userId: 'a' } }), false)
  assert.equal(mayJoinAfterInstall({ owner: null, relay: 'private' }), false)
})

test('the sidebar line says when this computer and the signed-in account differ', () => {
  const owned = { ...host, cloud: { enabled: true, owner: { userId: 'a', email: 'a@example.com' } } }
  assert.deepEqual(accountMismatch(owned, 'b'), { email: 'a@example.com' })
  assert.equal(accountMismatch(owned, 'a'), null, 'the same account')
  assert.equal(accountMismatch(host, 'b'), null, 'in no account is a choice')
  assert.equal(accountMismatch({ ...host, cloud: { enabled: false } }, 'b'), null)
  assert.equal(accountMismatch({ ...owned, online: false }, 'b'), null, "an offline host can't say")
  assert.equal(accountMismatch({ ...host, cloud: null }, 'b'), null)
})
