import assert from 'node:assert/strict'
import { test } from 'node:test'
import { agentModelsRequest, agentModelsScopeKey, normalizeLoginProfile } from './agent-models-query.ts'

test('empty, null, and undefined loginProfile normalize to null and omit loginProfile from agentModels request', () => {
  for (const empty of [undefined, null, '']) assert.equal(normalizeLoginProfile(empty), null)
  assert.deepEqual(agentModelsRequest('antigravity', null), { backend: 'antigravity' })
})

test('agentModelsScopeKey and agentModelsRequest distinguish each loginProfile for the same backend', () => {
  const defaultKey = agentModelsScopeKey('comp-1', 'antigravity', null)
  const proKey = agentModelsScopeKey('comp-1', 'antigravity', 'pro-7')
  const ultraKey = agentModelsScopeKey('comp-1', 'antigravity', 'ultra-8')

  assert.notEqual(defaultKey, proKey)
  assert.notEqual(proKey, ultraKey)

  assert.deepEqual(agentModelsRequest('antigravity', 'pro-7'), {
    backend: 'antigravity',
    loginProfile: 'pro-7',
  })
  assert.deepEqual(agentModelsRequest('antigravity', 'ultra-8'), {
    backend: 'antigravity',
    loginProfile: 'ultra-8',
  })
})
