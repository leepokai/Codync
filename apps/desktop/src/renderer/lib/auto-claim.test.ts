import assert from 'node:assert/strict'
import { test } from 'node:test'
import { mayAutoClaim } from './auto-claim.ts'

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
