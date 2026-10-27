import assert from 'node:assert/strict'
import { test } from 'node:test'
import { normalizeBot } from './models.ts'

test('an older host and a session without a report stay unknown, not zero', () => {
  assert.equal(normalizeBot({ id: 'b' }).contextUsage, null)
  assert.equal(normalizeBot({ id: 'b', contextUsage: null }).contextUsage, null)
})

test('the host context reading survives sync, including a real zero and compaction', () => {
  for (const usedTokens of [160000, 0, 20000]) {
    const contextUsage = { usedTokens, maxTokens: 200000, percent: usedTokens / 2000, level: usedTokens >= 160000 ? 'warning' : 'normal' }
    assert.deepEqual(normalizeBot(JSON.parse(JSON.stringify({ id: 'b', contextUsage }))).contextUsage, contextUsage)
  }
})
