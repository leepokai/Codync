import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computerSections, connectionSummary, moveInOrder, movePreservingFrozenLead, orderedComputers, rosterBand, rosterOrder } from './computer-roster.ts'

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

test('moving across a frozen lead keeps the lead in its slot; the input list is untouched', () => {
  const ids = ['a', 'lead', 'b']
  assert.deepEqual(movePreservingFrozenLead(ids, 'a', 'b', 'lead'), ['b', 'lead', 'a'])
  assert.deepEqual(movePreservingFrozenLead(ids, 'b', 'a', 'lead'), ['b', 'lead', 'a'])
  assert.deepEqual(movePreservingFrozenLead(['lead', 'a', 'b'], 'a', 'b', 'lead'), ['lead', 'b', 'a'])
  // Moving the lead itself is refused before this runs; the guard keeps it anyway.
  assert.deepEqual(movePreservingFrozenLead(ids, 'lead', 'b', 'lead'), ['a', 'lead', 'b'])
  assert.deepEqual(movePreservingFrozenLead(ids, 'b', 'lead', 'lead'), ['a', 'lead', 'b'], 'a drop onto the lead is refused too')
  assert.notEqual(movePreservingFrozenLead(ids, 'lead', 'b', 'lead'), ids, 'the refused path still returns a fresh list')
  // Unfrozen (no lead id), the plain move applies - the lead rides along, which is what the freeze prevents.
  assert.deepEqual(movePreservingFrozenLead(ids, 'a', 'b', undefined), ['lead', 'b', 'a'])
  assert.deepEqual(movePreservingFrozenLead(ids, 'b', 'a', undefined), ['b', 'a', 'lead'], 'the unfrozen fallback is a plain move both ways')
  // A lead in the last slot resplices at the end.
  assert.deepEqual(movePreservingFrozenLead(['a', 'b', 'lead'], 'b', 'a', 'lead'), ['b', 'a', 'lead'])
  assert.deepEqual([...ids], ['a', 'lead', 'b'])
})

test('pinned bots sort first; the lead is placed like any bot', () => {
  const bot = (id: string, over: Partial<{ lead: boolean; pinned: boolean; position: number; lastAt: number }> = {}) =>
    ({ id, lead: false, pinned: false, position: 0, lastAt: 0, ...over })
  const bots = [bot('recent', { lastAt: 9 }), bot('pinned', { pinned: true, position: -5 }), bot('lead', { lead: true, position: 3 }), bot('dragged', { position: -1 })]
  assert.deepEqual([...bots].sort(rosterOrder).map((b) => b.id), ['pinned', 'dragged', 'recent', 'lead'])
  assert.equal(rosterBand(bot('lead', { lead: true })), rosterBand(bot('other')), 'a drag moves the lead among the rest')
})

test('the header sums up the computers it shows, a cloud computer on its way as waking', () => {
  const tr = (text: string) => text
  const asleep = { kind: 'computerOffline', lastSeen: null } as const
  assert.equal(connectionSummary([{ kind: 'online' }, asleep], false, tr), '1/2 connected')
  assert.equal(connectionSummary([asleep], false, tr), '1 offline')
  assert.equal(connectionSummary([{ ...asleep, waking: true }], true, tr), 'Waking the computer…')
  assert.equal(connectionSummary([{ kind: 'connecting' }, asleep], true, tr), 'Connecting…')
})
