import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
import type { BotStore } from '../store/bot-store'
import type { ActionItem } from './DesktopActionMenu'
import { moveToSectionItems, sectionHeaderItems, type SectionNaming } from './section-menus.ts'

/** A computer's store as the menus read it (`archived` is hidden, its section still counts), logging what they ask of it. */
function computer(hasRosterSections = true) {
  const calls: unknown[][] = []
  const bots = [{ id: 'work', section: 'Work' }, { id: 'loose' }, { id: 'home', section: 'Home' }, { id: 'archived', section: 'archive', hidden: true }] as Bot[]
  const store = {
    hasRosterSections,
    bots: new Map(bots.map((bot) => [bot.id, bot])),
    sections: { move: (bot: Bot, name: string | null) => calls.push(['move', bot.id, name]) },
  } as unknown as BotStore
  const naming: SectionNaming[] = []
  return { store, calls, naming, bot: (id: string) => store.bots.get(id)! }
}

/** The "Move to section" submenu. */
function submenu(items: ActionItem[]) {
  assert.equal(items.length, 1)
  const [move] = items
  assert.ok(move && 'submenu' in move && move.title === 'Move to section')
  return move.submenu
}

test('Move to section offers the other sections (hidden bots\' too), a new one, and out of its own', () => {
  const { store, calls, naming, bot } = computer()
  const rows = submenu(moveToSectionItems(bot('work'), store, (n) => naming.push(n)))
  assert.deepEqual(rows.map((r) => r.title), ['archive', 'Home', 'New section…', 'Remove from section'])
  // Section names show as the user typed them ("Home" stays "Home" in any language); the rest is translated.
  assert.deepEqual(rows.map((r) => r.verbatim === true), [true, true, false, false])
  rows[1]!.action()
  rows[3]!.action()
  rows[2]!.action()
  assert.deepEqual(naming.map((n) => [n.title, n.name]), [['New section', '']])
  naming[0]!.save('Side')
  assert.deepEqual(calls, [['move', 'work', 'Home'], ['move', 'work', null], ['move', 'work', 'Side']])
})

test('a bot in no section has nothing to be removed from', () => {
  const { store, bot } = computer()
  assert.deepEqual(submenu(moveToSectionItems(bot('loose'), store, () => {})).map((r) => r.title), ['archive', 'Home', 'Work', 'New section…'])
})

test('a header renames its section from its name, or removes it', () => {
  const { store, calls, naming } = computer()
  const items = sectionHeaderItems('Work', store, (n) => naming.push(n), (to) => calls.push(['rename', to]))
  assert.deepEqual(items.map((i) => i.title), ['Rename section…', 'Remove section'])
  items[0]!.action()
  assert.deepEqual(naming.map((n) => [n.title, n.name]), [['Rename section', 'Work']])
  naming[0]!.save('Jobs')
  items[1]!.action()
  assert.deepEqual(calls, [['rename', 'Jobs'], ['rename', null]])
})

test('the lead moves to a section like any bot', () => {
  const { store, bot } = computer()
  assert.equal(moveToSectionItems({ ...bot('work'), lead: true }, store, () => {}).length, 1)
})

test('a host without sections gets no section menus', () => {
  const { store, bot } = computer(false)
  assert.deepEqual(moveToSectionItems(bot('work'), store, () => {}), [])
  assert.deepEqual(sectionHeaderItems('Work', store, () => {}, () => {}), [])
})
