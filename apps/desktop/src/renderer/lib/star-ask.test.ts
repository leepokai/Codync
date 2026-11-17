import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SECOND_ASK_AT, afterStarAsk, dueStarAsk } from './star-ask.ts'

test('asks right away, then once more at the fifth prompt, then never', () => {
  assert.equal(dueStarAsk({ kind: 'never' }, 0), 'first')
  const asked = afterStarAsk({ kind: 'never' }, false, 0)
  assert.deepEqual(asked, { kind: 'asked' })
  assert.equal(dueStarAsk(asked, SECOND_ASK_AT - 1), null)
  assert.equal(dueStarAsk(asked, SECOND_ASK_AT), 'second')
  assert.deepEqual(afterStarAsk(asked, false, SECOND_ASK_AT), { kind: 'done' })
  assert.equal(dueStarAsk({ kind: 'done' }, Number.MAX_SAFE_INTEGER), null)
})

test('starring ends the asks', () => {
  assert.deepEqual(afterStarAsk({ kind: 'never' }, true, 0), { kind: 'done' })
})

test('reopening setup keeps the second ask waiting', () => {
  const skipped = afterStarAsk({ kind: 'never' }, false, 0)
  assert.equal(dueStarAsk(skipped, 1), null)
  assert.deepEqual(afterStarAsk(skipped, false, 1), skipped)
  assert.equal(dueStarAsk(skipped, SECOND_ASK_AT), 'second')
})

test('starring from reopened setup stays final', () => {
  const skipped = afterStarAsk({ kind: 'never' }, false, 0)
  const starred = afterStarAsk(skipped, true, 1)
  assert.deepEqual(starred, { kind: 'done' })
  assert.deepEqual(afterStarAsk(starred, false, 2), starred)
  assert.equal(dueStarAsk(starred, SECOND_ASK_AT), null)
})
