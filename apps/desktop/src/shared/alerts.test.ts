import assert from 'node:assert/strict'
import test from 'node:test'
import { alertAnalyticsKind, parseAlert } from './alerts.ts'

test('an alert from the host reads back, and anything else is ignored', () => {
  const alert = { botId: 'b1', kind: 'needsInput', title: 'Builder', body: 'Which branch?' }
  assert.deepEqual(parseAlert(alert), alert)
  assert.deepEqual(parseAlert({ ...alert, body: undefined }), { ...alert, body: '' })
  assert.deepEqual(parseAlert({ ...alert, workflowId: 'w1' }), { ...alert, workflowId: 'w1' }, 'a workflow step\'s alert opens its workflow')
  assert.equal(parseAlert({ ...alert, kind: 'progress' }), null, 'a kind a newer host adds is not shown')
  assert.equal(parseAlert({ kind: 'done', title: 'No bot' }), null)
  assert.equal(parseAlert(null), null)
})

test('the analytics value of each kind is the catalog one', () => {
  assert.deepEqual((['done', 'needsInput', 'failed'] as const).map(alertAnalyticsKind), ['done', 'needs_input', 'failed'])
})
