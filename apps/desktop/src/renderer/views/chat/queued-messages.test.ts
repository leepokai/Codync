import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canSteer, outsideQueue, waitingMessages } from './queued-messages.ts'
import { withoutRemoved, type Entry, type EntryData } from '../../../shared/models.ts'

const user = (id: string, status: string, data: EntryData = {}): Entry => ({
  id, seq: 0, botId: 'b', rev: 1, kind: 'user', turn: 1, data: { text: id, status, ...data }, createdAt: 0, updatedAt: 0,
})

test('while the bot works, queued messages leave the chat for the queue, oldest first', () => {
  const entries = [user('sent', 'sent'), user('q1', 'queued'), user('q2', 'queued'), user('local', 'sending'), user('failed', 'failed')]
  assert.deepEqual(waitingMessages(entries, true).map((e) => e.id), ['q1', 'q2'])
  assert.deepEqual(outsideQueue(entries, true).map((e) => e.id), ['sent', 'local', 'failed'])
})

test('an idle bot takes its next message at once: the chat keeps it and the queue stays empty', () => {
  const entries = [user('q', 'queued')]
  assert.deepEqual(waitingMessages(entries, false), [])
  assert.deepEqual(outsideQueue(entries, false).map((e) => e.id), ['q'])
})

test('a message taken back is gone from what the views read, in one array per list', () => {
  const list = [user('removed', 'cancelled', { removed: true }), user('stopped', 'cancelled')]
  assert.deepEqual(withoutRemoved(list).map((e) => e.id), ['stopped'], 'one Stop dropped still says it wasn’t sent')
  assert.equal(withoutRemoved(list), withoutRemoved(list))
})

test('only a host that steers gets the queue’s buttons', () => {
  assert.equal(canSteer(undefined), false)
  assert.equal(canSteer('2.38.0'), false)
  assert.equal(canSteer('2.39.0'), true)
  assert.equal(canSteer('2.40.2'), true)
})
