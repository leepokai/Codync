import assert from 'node:assert/strict'
import { test } from 'node:test'
import { providerRegistry, usageStatus, windowTitle } from './usage-presentation.ts'

test('isolated Antigravity accounts keep the provider icon', () => {
  assert.equal(providerRegistry({ id: 'antigravity:acc_one' }), 'antigravity-acp')
  assert.equal(providerRegistry({ id: 'antigravity:acc_two' }), 'antigravity-acp')
  assert.equal(providerRegistry({ id: 'future:account' }), 'future')
})

test('model groups survive quota title formatting', () => {
  const window = { id: 'quota', percent: 4, resetsAt: null }
  assert.equal(windowTitle({ ...window, label: 'Gemini Models · Weekly Limit' }), 'Gemini Models · Weekly Limit')
  assert.equal(windowTitle({ ...window, label: 'Claude and GPT models · Weekly Limit' }), 'Claude and GPT models · Weekly Limit')
  assert.equal(windowTitle({ ...window, label: 'Weekly · Opus' }), 'Opus')
})

test('an unavailable account has an explanation instead of a quota number', () => {
  const provider = { id: 'antigravity', name: 'Antigravity', source: 'antigravity', updatedAt: 1, windows: [] }
  assert.equal(usageStatus({ ...provider, status: 'unsupported' }), 'Update this agent to read its plan limits.')
  assert.equal(usageStatus({ ...provider, status: 'unavailable' }), 'Plan limits are unavailable for this account.')
  assert.equal(usageStatus(provider), null)
})
