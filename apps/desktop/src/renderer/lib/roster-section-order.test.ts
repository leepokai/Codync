import assert from 'node:assert/strict'
import { test } from 'node:test'
import { moveInOrder } from './computer-roster.ts'
import { orderedSections, renamedSectionOrder } from './roster-section-order.ts'
import { rosterGroups, rosterLines, shownRows } from './roster-sections.ts'

test('whole sections move both ways across Bots while pinned rows stay first and navigation follows', () => {
  const item = (id: string, section?: string, pinned = false) => ({ bot: { id, section, pinned, unread: 0 } })
  const items = [item('pin', 'Work', true), item('a', 'Work'), item('b', 'Work'), item('loose')]
  const order = moveInOrder(['Work', ''], 'Work', '')
  assert.deepEqual(shownRows(rosterLines(items, [], order)).map((i) => i.bot.id), ['pin', 'loose', 'a', 'b'])
  assert.deepEqual(shownRows(rosterLines(items, [], moveInOrder(order, 'Work', ''))).map((i) => i.bot.id), ['pin', 'a', 'b', 'loose'])
  assert.deepEqual(shownRows(rosterLines(items, ['Work'], order)).map((i) => i.bot.id), ['pin', 'loose'])
})

test('saved order ignores absent groups and appends new groups once', () => {
  assert.deepEqual(orderedSections(['Home', 'Work', ''], ['Work', 'missing', '', 'Work']), ['Work', '', 'Home'])
})

test('renaming keeps position and merging keeps the destination position', () => {
  assert.deepEqual(renamedSectionOrder(['Work', '', 'Home'], 'Work', 'Play'), ['Play', '', 'Home'])
  assert.deepEqual(renamedSectionOrder(['Work', '', 'Home'], 'Work', 'Home'), ['', 'Home'])
  assert.deepEqual(renamedSectionOrder(['Work', ''], 'Work', null), [''])
  assert.deepEqual(renamedSectionOrder(['Work', ''], 'Work', 'Work'), ['Work', ''])
})

test('merge into a newly created group keeps the destination in its displayed position', () => {
  const bots = [{ section: 'Work' }, {}, { section: 'Home' }].map((bot) => ({ bot: { ...bot, pinned: false, unread: 0 } }))
  const displayed = rosterGroups(bots, [], ['Work', '']).map((group) => group.name)
  assert.deepEqual(renamedSectionOrder(displayed, 'Work', 'Home'), ['', 'Home'])
})
