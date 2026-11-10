import assert from 'node:assert/strict'
import { test } from 'node:test'
import { decodeRuns, decodeWorkflows, runFailed, runLabel } from './n8n-workflows.ts'

test('workflows keep their switch and drop what has no id', () => {
  const list = decodeWorkflows({
    workflows: [{ id: '1', name: 'Daily report', active: true }, { id: '2', name: 'Draft' }, { name: 'No id', active: true }],
    more: true,
  })
  assert.deepEqual(list.workflows, [{ id: '1', name: 'Daily report', active: true }, { id: '2', name: 'Draft', active: false }])
  assert.equal(list.more, true)
  assert.deepEqual(decodeWorkflows({}), { workflows: [], more: false })
  assert.deepEqual(decodeWorkflows(undefined), { workflows: [], more: false })
})

test('runs keep their time and name their status', () => {
  const runs = decodeRuns({
    executions: [
      { id: '1000', workflowId: '7', status: 'success', startedAt: '2026-11-01T10:00:00.000Z' },
      { id: '1001', workflowId: null, status: 'crashed' },
      { workflowId: '7', status: 'success' },
    ],
  })
  assert.equal(runs.length, 2)
  assert.equal(runs[0].startedAt, '2026-11-01T10:00:00.000Z')
  assert.equal(runs[1].workflowId, null)
  assert.equal(runs[1].startedAt, null)
  assert.deepEqual(decodeRuns(undefined), [])
})

test('a status reads as the phone does, and a failure is marked', () => {
  assert.equal(runLabel('success'), 'Succeeded')
  assert.equal(runLabel('new'), 'Waiting in the queue')
  assert.equal(runLabel('something-new'), 'Unknown')
  assert.equal(runFailed('error') && runFailed('crashed'), true)
  assert.equal(runFailed('success'), false)
})
