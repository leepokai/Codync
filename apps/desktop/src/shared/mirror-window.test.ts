import assert from 'node:assert/strict'
import { test } from 'node:test'
import { canLoadHistory, isOutsideLoadedWindow, mainChatFloor, snapshotFloors } from './mirror-window.ts'
import type { Entry } from './models.ts'

const entry = (id: string, seq: number, extra: Partial<Entry> = {}): Entry => ({ id, seq, botId: 'b', rev: 1, kind: 'notice', turn: 0, createdAt: 0, updatedAt: 0, data: {}, ...extra })

test('earlier main-chat history stays available after a catch-up dominated by replies', () => {
  const catchUp = Array.from({ length: 200 }, (_, i) => entry(`e${i}`, i + 100, { threadId: i < 6 ? null : 'root' }))
  assert.equal(canLoadHistory(catchUp, false), true)
  assert.equal(canLoadHistory(catchUp.filter((e) => e.threadId), false), true)
  assert.equal(canLoadHistory([entry('one', 100)], false), true)
  assert.equal(canLoadHistory(catchUp, true), false)
})

test('empty chats and unsent optimistic messages do not offer server history', () => {
  assert.equal(canLoadHistory([], false), false)
  assert.equal(canLoadHistory([entry('local-draft', Number.MAX_SAFE_INTEGER)], false), false)
  assert.equal(canLoadHistory([entry('local-draft', 0)], false), false)
})

test('a rewritten old entry below the floor is dropped, even with turn 0', () => {
  assert.equal(isOutsideLoadedWindow(100, false, entry('old', 40)), true)
})

test('an update to a held entry applies', () => {
  assert.equal(isOutsideLoadedWindow(100, true, entry('a', 40, { rev: 5 })), false)
})

test('a new entry at or above the floor applies', () => {
  assert.equal(isOutsideLoadedWindow(100, false, entry('mid', 102)), false)
  assert.equal(isOutsideLoadedWindow(100, false, entry('edge', 100)), false)
})

test('thread entries never fall outside', () => {
  assert.equal(isOutsideLoadedWindow(100, false, entry('r', 40, { threadId: 'root' })), false)
})

test('no floor accepts everything, including lower seqs after higher ones (rev-ordered catch-up)', () => {
  const floors = snapshotFloors(new Map(), 0)
  assert.equal(floors.get('b'), undefined)
  assert.equal(isOutsideLoadedWindow(floors.get('b'), false, entry('high', 90)), false)
  assert.equal(isOutsideLoadedWindow(floors.get('b'), false, entry('low', 10)), false)
})

test('since 0 snapshots nothing even when the mirror is loaded', () => {
  assert.equal(snapshotFloors(new Map([['b', [entry('a', 100)]]]), 0).size, 0)
})

test('floor ignores optimistic, threaded and unsequenced entries', () => {
  const list = [entry('local-n', 0), entry('local-m', 3), entry('t', 5, { threadId: 'x' }), entry('a', 100), entry('c', 120)]
  assert.equal(mainChatFloor(list, 7), 100)
  assert.equal(snapshotFloors(new Map([['b', list]]), 7).get('b'), 100)
  assert.equal(mainChatFloor([entry('local-n', 0)], 7), undefined)
})

test('a floor taken at hello is not moved by later inserts (RPC upsert before catch-up)', () => {
  const floors = snapshotFloors(new Map([['b', [entry('a', 100)]]]), 7)
  // A sent message's response lands at a high seq; catch-up entries between still apply.
  assert.equal(isOutsideLoadedWindow(floors.get('b'), false, entry('sent', 130)), false)
  assert.equal(isOutsideLoadedWindow(floors.get('b'), false, entry('caught', 110)), false)
  assert.equal(isOutsideLoadedWindow(floors.get('b'), false, entry('old', 20)), true)
})

test("a send's RPC response newer than since doesn't set the floor", () => {
  const floors = snapshotFloors(new Map([['b', [entry('sent', 1010, { rev: 9 })]]]), 7)
  assert.equal(floors.get('b'), undefined)
  assert.equal(isOutsideLoadedWindow(floors.get('b'), false, entry('routine', 1000, { rev: 8 })), false)
})
