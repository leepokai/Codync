import assert from 'node:assert/strict'
import { test } from 'node:test'
import { foldsAfterRename, limitSectionName, placedBots, rosterLines, sameRosterGroup, sectionMembers, sectionNames, sectionRename, shownRows, type RosterLine } from './roster-sections.ts'

const item = (id: string, section?: string, { pinned = false, unread = 0 } = {}) => ({ id, bot: { pinned, section, unread } })
type Item = ReturnType<typeof item>

/** Each line as text: a row's id, the "Bots" label, or a header's name, count, unread and fold. */
const outline = (lines: RosterLine<Item>[]) =>
  lines.map((line) => (line.kind === 'row' ? line.item.id : line.kind === 'label' ? 'BOTS'
    : `${line.name} ${line.count} ${line.unread}${line.folded ? ' folded' : ''}: ${line.items.map((i) => i.id).join(' ')}`))

test('pinned rows first, then sections by name, then rows without a section under the label', () => {
  const roster = [
    item('pinned-work', 'Work', { pinned: true }),
    item('loose'),
    item('work-1', 'Work', { unread: 2 }),
    item('home', 'personal'),
    item('work-2', 'Work', { unread: 3 }),
    item('apps', 'apps'),
  ]
  assert.deepEqual(outline(rosterLines(roster, [])), [
    'pinned-work',
    'apps 1 0: apps',
    'personal 1 0: home',
    'Work 2 5: work-1 work-2',
    'BOTS',
    'loose',
  ])
})

test('a folded section keeps its rows for the fold, but only its header is on screen', () => {
  const roster = [item('a', 'Work'), item('b', 'Work'), item('c')]
  const lines = rosterLines(roster, ['Work'])
  assert.deepEqual(outline(lines), ['Work 2 0 folded: a b', 'BOTS', 'c'])
  assert.deepEqual(shownRows(lines).map((i) => i.id), ['c'])
  assert.deepEqual(shownRows(rosterLines(roster, [])).map((i) => i.id), ['a', 'b', 'c'])
})

test('a section only its pinned bots are in has no header, and no header means no label', () => {
  assert.deepEqual(outline(rosterLines([item('p', 'Work', { pinned: true }), item('c')], [])), ['p', 'c'])
})

test('the label shows only with a section header and a bot in none', () => {
  assert.deepEqual(outline(rosterLines([item('a'), item('b')], [])), ['a', 'b'])
  assert.deepEqual(outline(rosterLines([item('p', undefined, { pinned: true }), item('a', 'Work')], [])), ['p', 'Work 1 0: a'])
})

test('a bot moves past bots with the same pin state and section; pinned bots ignore sections', () => {
  const bot = (section?: string, pinned = false) => ({ pinned, section })
  assert.equal(sameRosterGroup(bot('Work'), bot('Work')), true)
  assert.equal(sameRosterGroup(bot('Work'), bot('Home')), false)
  assert.equal(sameRosterGroup(bot('Work'), bot()), false)
  assert.equal(sameRosterGroup(bot(), bot('')), true)
  assert.equal(sameRosterGroup(bot('Work', true), bot('Home', true)), true)
  assert.equal(sameRosterGroup(bot('Work', true), bot('Work')), false)
})

test('every section once, sorted by name', () => {
  assert.deepEqual(sectionNames([{ section: 'Work' }, {}, { section: 'apps' }, { section: 'Work' }, { section: '' }]), ['apps', 'Work'])
})

test('removing a section sends `to` as null, which JSON keeps', () => {
  assert.equal(JSON.stringify(sectionRename('Work', null)), '{"from":"Work","to":null}')
  assert.equal(JSON.stringify(sectionRename('Work', '  ')), '{"from":"Work","to":null}')
  assert.deepEqual(sectionRename('Work', ' Jobs '), { from: 'Work', to: 'Jobs' })
})

test('a name field stops at 40 characters as the host counts them, never inside an emoji', () => {
  const forty = 'a'.repeat(40)
  assert.equal(limitSectionName(forty + 'b'), forty)
  assert.equal(limitSectionName('😀'.repeat(41)), '😀'.repeat(40))
  assert.equal(limitSectionName('a'.repeat(39) + '😀b'), 'a'.repeat(39) + '😀')
})

test('a folded section renamed or removed here takes its fold along; an unfolded one changes nothing', () => {
  assert.deepEqual(foldsAfterRename(['Home', 'Work'], 'Work', 'Jobs'), ['Home', 'Jobs'])
  assert.deepEqual(foldsAfterRename(['Home'], 'Work', 'Jobs'), ['Home'])
  assert.deepEqual(foldsAfterRename(['Home'], 'Work', 'Home'), ['Home']) // Never unfolds the target.
  assert.deepEqual(foldsAfterRename(['Home', 'Work'], 'Work', null), ['Home'])
  assert.deepEqual(foldsAfterRename(['Work'], 'Work', 'Home'), ['Home']) // A merge folds the section it joins.
  assert.deepEqual(foldsAfterRename(['Home', 'Work'], 'Work', 'Home'), ['Home'])
})

type Placeable = { id: string; section?: string; unread: number }
/** A computer's bots by id. */
const computerBots = (...bots: Placeable[]) => new Map(bots.map((bot) => [bot.id, bot]))
/** Shows placed bots as `BotStore.place` does. */
const apply = (bots: Map<string, Placeable>, placed: [string, Placeable][]) => {
  for (const [id, bot] of placed) bots.set(id, bot)
}

test('a refused section change for one bot puts back only its section', () => {
  const bots = computerBots({ id: 'a', section: 'Work', unread: 0 }, { id: 'b', section: 'Work', unread: 0 })
  const before = bots.get('a')?.section // setSection remembers it…
  bots.set('a', { id: 'a', section: 'Home', unread: 3 }) // …shows the move, and a message arrives meanwhile.
  const undo = placedBots(bots, ['a'], before)
  assert.deepEqual(undo, [['a', { id: 'a', section: 'Work', unread: 3 }]])
  apply(bots, undo)
  assert.deepEqual([...bots.values()], [{ id: 'a', section: 'Work', unread: 3 }, { id: 'b', section: 'Work', unread: 0 }])
})

test('a refused rename puts back exactly the bots it moved, not those already in the target', () => {
  const bots = computerBots({ id: 'w1', section: 'Work', unread: 0 }, { id: 'h', section: 'Home', unread: 0 }, { id: 'w2', section: 'Work', unread: 0 }, { id: 'n', unread: 0 })
  for (const to of ['Home', undefined]) { // Merged into Home, or removed.
    const moved = sectionMembers(bots.values(), 'Work')
    assert.deepEqual(moved, ['w1', 'w2'])
    apply(bots, placedBots(bots, moved, to))
    assert.deepEqual([...bots.values()].map((bot) => bot.section), [to, 'Home', to, undefined])
    apply(bots, placedBots(bots, moved, 'Work'))
    assert.deepEqual([...bots.values()].map((bot) => bot.section), ['Work', 'Home', 'Work', undefined])
  }
})

test('a bot deleted before the refusal comes back stays deleted', () => {
  assert.deepEqual(placedBots(computerBots({ id: 'a', unread: 0 }), ['gone', 'a'], 'Work'), [['a', { id: 'a', section: 'Work', unread: 0 }]])
})
