import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from './models.ts'
import { isTold, mergeTaskEntries, parseTaskEntry } from './tasks.ts'

const entry = (id: string, seq: number, rev: number, text = id): Entry => ({ id, seq, botId: 'task-1', rev, kind: 'agent', turn: 1, data: { text }, createdAt: 0, updatedAt: 0 })

test('a taskEntry event names its task and carries one entry', () => {
  const parsed = parseTaskEntry({ type: 'taskEntry', rev: 7, task: 'task-1', entry: { id: 'e1', seq: 1, botId: 'task-1', rev: 7, kind: 'tool', turn: 1 } })
  assert.equal(parsed?.task, 'task-1')
  assert.deepEqual(parsed?.entry.data, {}, 'an entry without data still reads')
  assert.equal(parseTaskEntry({ type: 'taskEntry', entry: { id: 'e1' } }), null, 'no task')
  assert.equal(parseTaskEntry({ type: 'taskEntry', task: 'task-1' }), null, 'no entry')
})

test('a task\'s transcript holds each entry once, the higher rev, oldest first', () => {
  const fetched = [entry('a', 1, 1), entry('b', 2, 2, 'old')]
  const live = mergeTaskEntries(fetched, [entry('c', 3, 5), entry('b', 2, 4, 'new')])
  assert.deepEqual(live.map((e) => [e.id, e.data.text]), [['a', 'a'], ['b', 'new'], ['c', 'c']])
  // A fetch that answers after a live update doesn't undo it.
  assert.equal(mergeTaskEntries(live, [entry('b', 2, 2, 'old')]).find((e) => e.id === 'b')!.data.text, 'new')
  assert.equal(mergeTaskEntries(live, fetched).length, 3)
})

test('what the host told a task is not the user\'s own message', () => {
  assert.equal(isTold({ ...entry('a', 1, 1), kind: 'user', data: { text: 'brief', from: 'host' } }), true)
  assert.equal(isTold({ ...entry('a', 1, 1), kind: 'user', data: { text: 'typed' } }), false)
  assert.equal(isTold(entry('a', 1, 1)), false)
})
