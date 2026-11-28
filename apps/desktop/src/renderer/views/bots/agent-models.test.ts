import assert from 'node:assert/strict'
import { test } from 'node:test'
import { agentModelsRequest, normalizeLoginProfile } from './agent-models-query.ts'

test('empty, null, and undefined loginProfile normalize to null and omit loginProfile from agentModels request', () => {
  for (const empty of [undefined, null, '']) assert.equal(normalizeLoginProfile(empty), null)
  assert.deepEqual(agentModelsRequest('antigravity', null), { backend: 'antigravity' })
})

test('agentModelsRequest distinguishes each loginProfile for the same backend', () => {
  assert.deepEqual(agentModelsRequest('antigravity', 'pro-7'), {
    backend: 'antigravity',
    loginProfile: 'pro-7',
  })
  assert.deepEqual(agentModelsRequest('antigravity', 'ultra-8'), {
    backend: 'antigravity',
    loginProfile: 'ultra-8',
  })
})

test('a refresh asks the computer to read the agent again', () => {
  assert.deepEqual(agentModelsRequest('claude', null, true), { backend: 'claude', refresh: true })
  assert.deepEqual(agentModelsRequest('claude', 'p-1', false), { backend: 'claude', loginProfile: 'p-1' })
})
