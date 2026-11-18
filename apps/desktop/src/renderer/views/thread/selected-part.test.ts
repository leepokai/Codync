import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canReplyToPart } from './selected-part.ts'

test('only a host that takes a part of a message gets one', () => {
  assert.equal(canReplyToPart(undefined), false)
  assert.equal(canReplyToPart('2.48.0'), false, 'it would quote the whole message')
  assert.equal(canReplyToPart('2.48.1'), true)
  assert.equal(canReplyToPart('2.49.0'), true)
})
