import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computerSections, moveInOrder, orderedComputers, rosterBand, rosterOrder } from './computer-roster.ts'

test('saved computer order survives reconnect order and appends new computers', () => {
  const computers = [{ id: 'new' }, { id: 'a' }, { id: 'b' }]
  assert.deepEqual(orderedComputers(computers, ['gone', 'b', 'a']).map((c) => c.id), ['b', 'a', 'new'])
  assert.equal(computers[0]?.id, 'new')
})

test('moving a section works in either direction without losing hidden computers', () => {
  const order = ['a', 'hidden', 'b', 'c']
  assert.deepEqual(moveInOrder(order, 'a', 'b'), ['hidden', 'b', 'a', 'c'])
  assert.deepEqual(moveInOrder(order, 'c', 'a'), ['c', 'a', 'hidden', 'b'])
  assert.deepEqual(moveInOrder(order, 'a', 'a'), order)
  assert.deepEqual(moveInOrder(order, 'missing', 'b'), order)
  assert.deepEqual(moveInOrder(order, 'a', 'missing'), order)
  assert.deepEqual(order, ['a', 'hidden', 'b', 'c'])
})

test('each computer keeps its own bots in activity order, including duplicate bot ids', () => {
  const roster = [
    { id: 'same', ref: { computerId: 'b' } },
    { id: 'same', ref: { computerId: 'a' } },
    { id: 'hidden', ref: { computerId: 'hidden' } },
    { id: 'older', ref: { computerId: 'b' } },
  ]
  const sections = computerSections([{ id: 'a' }, { id: 'b' }, { id: 'empty' }], roster)
  assert.deepEqual(sections.map((s) => s.items.map((i) => i.id)), [['same'], ['same', 'older'], []])
  assert.equal(sections[0]?.items[0]?.ref.computerId, 'a')
})

test('pinned bots sort first; the lead is placed like any bot', () => {
  const bot = (id: string, over: Partial<{ lead: boolean; pinned: boolean; position: number; lastAt: number }> = {}) =>
    ({ id, lead: false, pinned: false, position: 0, lastAt: 0, ...over })
  const bots = [bot('recent', { lastAt: 9 }), bot('pinned', { pinned: true, position: -5 }), bot('lead', { lead: true, position: 3 }), bot('dragged', { position: -1 })]
  assert.deepEqual([...bots].sort(rosterOrder).map((b) => b.id), ['pinned', 'dragged', 'recent', 'lead'])
  assert.equal(rosterBand(bot('lead', { lead: true })), rosterBand(bot('other')), 'a drag moves the lead among the rest')
})
