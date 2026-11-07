import assert from 'node:assert/strict'
import { test } from 'node:test'
import { compactionLabel, compactionMenu, compactionSizes } from './compaction-options.ts'

test('a 1M window offers every 10K from 50K to 960K', () => {
  const sizes = compactionSizes({ min: 50_000, max: 960_000, step: 10_000 })
  assert.equal(sizes.length, 92)
  assert.deepEqual([sizes[0], sizes.at(-1)], [50_000, 960_000])
  assert.deepEqual(compactionSizes({ min: 50_000, max: 960_000, step: 0 }), [], 'a bad step lists nothing')
})

test('the menu checks the choice and opens the sizes', () => {
  const picked: unknown[] = []
  const menu = compactionMenu(120_000, { min: 50_000, max: 160_000, step: 10_000 }, (t) => picked.push(t))
  assert.deepEqual(menu.map((m) => m.title), ['Early (default)', 'Near the limit', 'Only when full', 'Set a size'])
  assert.ok(menu.slice(0, 3).every((m) => m.selected === false))
  const sizes = menu[3].submenu ?? []
  assert.deepEqual(sizes.map((m) => m.title), ['50K', '60K', '70K', '80K', '90K', '100K', '110K', '120K', '130K', '140K', '150K', '160K'])
  assert.deepEqual(sizes.filter((m) => m.selected).map((m) => m.title), ['120K'])
  sizes[0].action?.()
  menu[1].action?.()
  assert.deepEqual(picked, [50_000, 'nearLimit'])
})

test('before the window is known the sizes wait for the next reply', () => {
  const menu = compactionMenu('early', null, () => {})
  assert.equal(menu[0].selected, true)
  assert.deepEqual(menu.at(-1), { title: "Sizes show after the bot's next reply", disabled: true, divider: true, action: menu.at(-1)?.action })
})

test('the pill shows a preset translated and a size as it is', () => {
  assert.deepEqual(compactionLabel('off'), { title: 'Only when full', verbatim: false })
  assert.deepEqual(compactionLabel(300_000), { title: '300K', verbatim: true })
})
