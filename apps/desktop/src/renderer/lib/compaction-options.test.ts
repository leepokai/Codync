import assert from 'node:assert/strict'
import { test } from 'node:test'
import { compactionLabel, compactionMenu, fitSize, parseTypedSize } from './compaction-options.ts'

const million = { min: 50_000, max: 960_000, step: 10_000, presets: [100_000, 200_000, 300_000] }

test('a typed size goes to the nearest 10K the window takes', () => {
  assert.deepEqual([354_000, 355_000, 10_000, 5_000_000].map((n) => fitSize(million, n)), [350_000, 360_000, 50_000, 960_000])
})

test('a size is typed in thousands of tokens', () => {
  assert.deepEqual(['350', '350k', ' 1.5K ', 'abc', '', '0', '-5'].map(parseTypedSize), [350_000, 350_000, 1500, null, null, null, null])
})

test('the menu lists the presets, then the sizes and a custom one', () => {
  const picked: unknown[] = []
  let custom = 0
  const menu = compactionMenu(200_000, million, (t) => picked.push(t), () => custom++)
  assert.deepEqual(menu.map((m) => m.title), ['Auto (default)', 'Early', 'Near the limit', 'Set a size'])
  assert.ok(menu.slice(0, 3).every((m) => m.selected === false))
  const sizes = menu[3].submenu ?? []
  assert.deepEqual(sizes.map((m) => m.title), ['100K', '200K', '300K', 'Custom size…'])
  assert.deepEqual(sizes.filter((m) => m.selected).map((m) => m.title), ['200K'])
  sizes[0].action?.()
  menu[1].action?.()
  sizes.at(-1)?.action?.()
  assert.deepEqual(picked, [100_000, 'early'])
  assert.equal(custom, 1)
  const own = compactionMenu(350_000, million, () => {}, () => {})[3].submenu ?? []
  assert.deepEqual(own.filter((m) => m.selected).map((m) => m.title), ['Custom size…'], 'a size of the user\'s own')
})

test('before the window is known the sizes wait for the next reply', () => {
  const menu = compactionMenu('off', null, () => {}, () => {})
  assert.equal(menu[0].selected, true)
  assert.deepEqual(menu.at(-1), { title: "Sizes show after the bot's next reply", disabled: true, divider: true, action: menu.at(-1)?.action })
})

test('the pill shows a preset translated and a size as it is', () => {
  assert.deepEqual(compactionLabel('off'), { title: 'Auto (default)', verbatim: false })
  assert.deepEqual(compactionLabel(350_000), { title: '350K', verbatim: true })
})
