import assert from 'node:assert/strict'
import { test } from 'node:test'
import { maySendSetupFailure } from './analytics.ts'

test('a host setup failure is sent only when the owner can’t have turned analytics off', () => {
  assert.equal(maySendSetupFailure(true, true), true)
  assert.equal(maySendSetupFailure(undefined, false), true, 'a first install: nothing stored yet, so on')
  assert.equal(maySendSetupFailure(false, false), false)
  assert.equal(maySendSetupFailure(false, true), false)
  assert.equal(maySendSetupFailure(undefined, true), false, 'an older app never kept the host’s setting')
})
