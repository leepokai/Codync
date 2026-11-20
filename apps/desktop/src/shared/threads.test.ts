import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from './models.ts'
import { isTold, localEntryId, mergeThreadEntries, parseThreadEntry, unsentEntries } from './threads.ts'

const entry = (id: string, seq: number, rev: number, text = id): Entry => ({ id, seq, botId: 'thread-1', rev, kind: 'agent', turn: 1, data: { text }, createdAt: 0, updatedAt: 0 })

test('a threadEntry event names its thread and carries one entry', () => {
  const parsed = parseThreadEntry({ type: 'threadEntry', rev: 7, thread: 'thread-1', entry: { id: 'e1', seq: 1, botId: 'thread-1', rev: 7, kind: 'tool', turn: 1 } })
  assert.equal(parsed?.thread, 'thread-1')
  assert.deepEqual(parsed?.entry.data, {}, 'an entry without data still reads')
  assert.equal(parseThreadEntry({ type: 'threadEntry', entry: { id: 'e1' } }), null, 'no thread')
  assert.equal(parseThreadEntry({ type: 'threadEntry', thread: 'thread-1' }), null, 'no entry')
})

test('a thread\'s transcript holds each entry once, the higher rev, oldest first', () => {
  const fetched = [entry('a', 1, 1), entry('b', 2, 2, 'old')]
  const live = mergeThreadEntries(fetched, [entry('c', 3, 5), entry('b', 2, 4, 'new')])
  assert.deepEqual(live.map((e) => [e.id, e.data.text]), [['a', 'a'], ['b', 'new'], ['c', 'c']])
  // A fetch that answers after a live update doesn't undo it.
  assert.equal(mergeThreadEntries(live, [entry('b', 2, 2, 'old')]).find((e) => e.id === 'b')!.data.text, 'new')
  assert.equal(mergeThreadEntries(live, fetched).length, 3)
})

test('what the host told a thread is not the user\'s own message', () => {
  assert.equal(isTold({ ...entry('a', 1, 1), kind: 'user', data: { text: 'brief', from: 'host' } }), true)
  assert.equal(isTold({ ...entry('a', 1, 1), kind: 'user', data: { text: 'typed' } }), false)
  assert.equal(isTold(entry('a', 1, 1)), false)
})

test("a message sent to a thread shows until the host's entry for it is in", () => {
  const local: Entry = { ...entry(localEntryId('n1'), Number.MAX_SAFE_INTEGER, 0, 'hello'), kind: 'user', data: { text: 'hello', status: 'sending', clientNonce: 'n1' } }
  const held = mergeThreadEntries([entry('a', 1, 1)], [local])
  assert.deepEqual(held.map((e) => e.id), ['a', 'local-n1'], 'last, whatever arrives meanwhile')
  // It didn't arrive: the same message, now failed.
  assert.equal(mergeThreadEntries(held, [{ ...local, data: { ...local.data, status: 'failed' } }]).at(-1)!.data.status, 'failed')
  const arrived: Entry = { ...entry('e9', 2, 9, 'hello'), kind: 'user', data: { text: 'hello', status: 'queued', clientNonce: 'n1' } }
  assert.deepEqual(mergeThreadEntries(held, [arrived]).map((e) => e.id), ['a', 'e9'])
  assert.deepEqual(mergeThreadEntries(held, [entry('b', 2, 2)]).map((e) => e.id), ['a', 'b', 'local-n1'], 'another entry leaves it')
})

test('a closed thread keeps only the messages that never reached it', () => {
  const local = (nonce: string, status: string, removed?: boolean): Entry => ({ ...entry(localEntryId(nonce), Number.MAX_SAFE_INTEGER, 0), kind: 'user', data: { text: nonce, status, clientNonce: nonce, removed } })
  const arrived: Entry = { ...entry('e9', 2, 9), kind: 'user', data: { text: 'hello', clientNonce: 'n0' } }
  const held = [entry('a', 1, 1), arrived, local('n1', 'sending'), local('n2', 'failed'), local('n3', 'failed', true)]
  assert.deepEqual(unsentEntries(held).map((e) => e.id), ['local-n1', 'local-n2'], 'the host\'s entries and a deleted one go')
})
