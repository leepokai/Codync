import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canSchedule, canSteer, heldOutside, outsideQueue, waitingMessages } from './queued-messages.ts'
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

test('a scheduled message is always in the tray, by its fire time, and never in the chat', () => {
  const entries = [user('late', 'scheduled', { notBefore: 5_000 }), user('soon', 'scheduled', { notBefore: 2_000 }), user('sent', 'sent')]
  assert.deepEqual(waitingMessages(entries, false).map((e) => e.id), ['soon', 'late'], "it shows while the bot isn't working")
  const entriesBusy = [user('sent', 'sent'), user('q', 'queued'), user('soon', 'scheduled', { notBefore: 2_000 })]
  assert.deepEqual(waitingMessages(entriesBusy, true).map((e) => e.id), ['q', 'soon'], 'the live queue first, scheduled at the tail')
  assert.deepEqual(outsideQueue(entriesBusy, true).map((e) => e.id), ['sent'], 'neither kind shows in the chat while it waits')
})

test('a scheduled message older than the loaded window still reaches the tray, from the host', () => {
  // The chat's window holds the newest entries only; `hello.held` lists every message still held.
  const entries = [user('sent', 'sent'), user('queued', 'queued')]
  const held = [user('old', 'scheduled', { notBefore: 1_000 }), { ...user('other', 'scheduled', { notBefore: 1_000 }), botId: 'c' }]
  assert.deepEqual(heldOutside(entries, 'b', held).map((e) => e.id), ['old'], 'this chat only')
  assert.deepEqual(waitingMessages([...entries, ...heldOutside(entries, 'b', held)], true).map((e) => e.id), ['queued', 'old'])
  // An entry the chat holds wins: an edit, a cancel or the fire itself lands there first.
  const edited = [user('old', 'scheduled', { notBefore: 9_000 })]
  assert.deepEqual(heldOutside(edited, 'b', held), [], 'the chat\u2019s own copy is the newer one')
  assert.deepEqual(heldOutside(entries, 'b', undefined), [], 'an older host sends none')
  // A row the chat has taken back is dropped from the shown list, so only the removal keeps its snapshot copy out.
  const takenBack = [user('gone', 'scheduled', { notBefore: 1_000 })]
  assert.deepEqual(heldOutside([], 'b', takenBack), takenBack, 'the snapshot still reaches the tray while nothing removed it')
  assert.deepEqual(heldOutside([], 'b', takenBack, new Set(['gone'])), [], 'a taken-back row does not come back')
})

test('a held id the chat list has already dropped never comes back to the tray', () => {
  // `hello.held` is a snapshot, so it still names a row the user took back after it was taken.
  const snapshot = [user('gone', 'scheduled', { notBefore: 1_000 }), user('kept', 'scheduled', { notBefore: 2_000 })]
  // The chat's rows: the hidden tombstone a take-back leaves, which every chat list drops (`allEntries`).
  const rows = [user('gone', 'cancelled', { removed: true, notBefore: 1_000 })]
  const entries = withoutRemoved(rows)
  assert.deepEqual(entries, [], 'the chat has dropped the row the snapshot still lists')
  // `BotMirror.takenBack`, read off those same rows.
  const takenBack = new Set(rows.filter((e) => e.data.removed).map((e) => e.id))
  assert.deepEqual(heldOutside(entries, 'b', snapshot).map((e) => e.id), ['gone', 'kept'], 'the raw snapshot re-adds it')
  const waiting = waitingMessages([...entries, ...heldOutside(entries, 'b', snapshot, takenBack)], false)
  assert.deepEqual(waiting.map((e) => e.id), ['kept'], 'the tray paints only the held rows the chat still shows')
})

test('only a host that schedules gets the schedule affordances', () => {
  assert.equal(canSchedule(undefined), false)
  assert.equal(canSchedule({ scheduleSend: true }), true)
  assert.equal(canSchedule({ scheduleSend: false }), false)
})
