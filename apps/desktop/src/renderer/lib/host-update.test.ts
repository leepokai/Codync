import assert from 'node:assert/strict'
import { test } from 'node:test'
import { HOST_UPDATE_WAIT, offerHostUpdate } from './host-update.ts'

test('an update asked from here runs for a while, then is offered again', () => {
  const requested = { kind: 'requested', at: 1_000_000 } as const
  assert.deepEqual(offerHostUpdate(null, true, requested.at), { kind: 'available', last: null })
  assert.deepEqual(offerHostUpdate(requested, true, requested.at + HOST_UPDATE_WAIT - 1), { kind: 'updating' })
  assert.deepEqual(offerHostUpdate(requested, true, requested.at + HOST_UPDATE_WAIT), { kind: 'available', last: requested })
})

test('a refused update is offered again with why; a computer that cannot update from here is not', () => {
  const failed = { kind: 'failed', message: 'A bot is working on this computer. Update it once the bot is done.' } as const
  assert.deepEqual(offerHostUpdate(failed, true), { kind: 'available', last: failed })
  assert.deepEqual(offerHostUpdate(null, false), { kind: 'unavailable' })
})
