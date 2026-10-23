import { test } from 'node:test'
import assert from 'node:assert/strict'
import { modelSettings } from './model-settings.ts'

const effort = { id: 'reasoning_effort', name: 'Reasoning Effort', default: 'low', values: [{ id: 'low', name: 'Low' }, { id: 'high', name: 'High' }] }
const catalog = {
  models: [{ id: 'gpt-6.1-sol', options: [effort] }, { id: 'gpt-6.1-mini', options: [] }],
  currentModelId: 'gpt-6.1-sol',
}

test('a bot on the Default model gets the settings of the model the agent runs by default', () => {
  assert.deepEqual(modelSettings(catalog, ''), [effort])
})

test('a chosen model shows its own settings', () => {
  assert.deepEqual(modelSettings(catalog, 'gpt-6.1-mini'), [])
  assert.deepEqual(modelSettings(catalog, 'gpt-6.1-sol'), [effort])
})

test('no catalog, an unlisted model or an unknown default shows no settings', () => {
  assert.deepEqual(modelSettings(null, ''), [])
  assert.deepEqual(modelSettings(catalog, 'typed-by-hand'), [])
  assert.deepEqual(modelSettings({ models: catalog.models }, ''), [])
})
