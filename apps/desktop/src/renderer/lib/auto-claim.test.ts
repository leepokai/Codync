import assert from 'node:assert/strict'
import { test } from 'node:test'
import { accountMismatch, mayAutoClaim, mayOfferMove } from './auto-claim.ts'

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

test('the sidebar line says when this computer and the signed-in account differ', () => {
  const owned = { ...host, cloud: { enabled: true, owner: { userId: 'a', email: 'a@example.com' } } }
  assert.deepEqual(accountMismatch(owned, 'b', [], []), { kind: 'other', email: 'a@example.com' })
  assert.equal(accountMismatch(owned, 'a', [], []), null, 'the same account')
  assert.deepEqual(accountMismatch(owned, 'b', ['new'], []), { kind: 'other', email: 'a@example.com' }, 'still shown after keep it there')
  assert.equal(accountMismatch(host, 'b', [], []), null, 'joining on its own')
  assert.deepEqual(accountMismatch({ ...host, cloud: { enabled: false } }, 'b', [], []), { kind: 'none', reason: 'cloudOff' })
  assert.deepEqual(accountMismatch({ ...host, cloud: { enabled: true, relay: 'private' } }, 'b', [], []), { kind: 'none', reason: 'privateRelay' })
  assert.deepEqual(accountMismatch(host, 'b', ['new'], []), { kind: 'none', reason: 'keptOut' })
  assert.deepEqual(accountMismatch(host, 'b', [], ['new']), { kind: 'none', reason: 'keptOut' }, 'removed from the account elsewhere')
  assert.equal(accountMismatch({ ...owned, online: false }, 'b', [], []), null, "an offline host can't say")
  assert.equal(accountMismatch({ ...host, cloud: null }, 'b', [], []), null)
})
