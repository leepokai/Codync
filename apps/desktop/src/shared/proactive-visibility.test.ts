import assert from 'node:assert/strict'
import { test } from 'node:test'
import { collectDeliverables, collectPendingDecisions, isChat, isInternalEntry, isReplyable, type Entry } from './models.ts'

function entry(kind: Entry['kind'], data: Entry['data'], seq = 1): Entry {
  return { id: `e${seq}`, botId: 'bot', seq, rev: seq, turn: 1, kind, data, createdAt: seq, updatedAt: seq }
}

test('scheduled prompts and provisional output never become chat or reply targets', () => {
  for (const kind of ['user', 'agent', 'thought', 'tool', 'plan', 'notice', 'permission'] as const) {
    const hidden = entry(kind, { internal: true, text: 'NO_UPDATE', final: true })
    assert.equal(isInternalEntry(hidden), true)
    assert.equal(isChat(hidden), false)
    assert.equal(isReplyable(hidden), false)
  }
  assert.equal(isChat(entry('user', { from: 'proactive', text: 'Review open items' })), false)
  assert.equal(isChat(entry('agent', { internal: false, text: 'Your build is ready.', final: true })), true)
  assert.equal(isChat(entry('user', { text: 'My own request' })), true)
})

test('internal activity neither settles a pending incident nor produces decisions or deliverables', () => {
  const incident = entry('notice', { style: 'error', text: 'Needs your attention' })
  const internal = entry('user', { internal: true, text: 'Review open items' }, 2)
  const permission = entry('permission', { internal: true, status: 'pending' }, 3)
  assert.deepEqual(collectPendingDecisions([incident, internal, permission]).map((d) => d.entryId), [incident.id])
  const hiddenFile = entry('agent', { internal: true, files: [{ id: 'file', name: 'Internal', size: 1, sha256: 'hash' }] })
  assert.deepEqual(collectDeliverables([hiddenFile]), [])
})
