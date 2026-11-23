import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { RosterItem } from '../store/app-model'
import type { AgentModels } from './bots/agent-models'
import type { ApplyPlan, CatalogCheck } from './bots/model-manager-logic'
import { allKeys, applyTargets, clampSheetSize, defaultSheetSize, filterChecked, hiddenCheckedCount, planRows, toggleKey } from './bots/model-manager-logic.ts'

/** A roster row carries a store, a bot and a computer; these tests only read ref, bot and computer. */
const item = (computerId: string, computerName: string, botId: string, name: string, description = ''): RosterItem =>
  ({
    ref: { computerId, botId },
    bot: { id: botId, name, description, lastMessage: null },
    computer: { id: computerId, name: computerName },
    store: { computer: { id: computerId } },
  }) as unknown as RosterItem

const catalog = (...ids: string[]): AgentModels => ({ models: ids.map((id) => ({ id, name: id })) })
const ok = (ids: string[]): CatalogCheck => ({ kind: 'ok', catalog: catalog(...ids) })
const no: CatalogCheck = { kind: 'none' }
const bad: CatalogCheck = { kind: 'error' }

test('allKeys names every bot by its computer and bot, not by bot alone', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha'), item('linux', 'Linux', 'b1', 'Alpha')]
  assert.deepEqual(allKeys(items), ['mac/b1', 'linux/b1'])
})

test('filterChecked keeps the checked bots in roster order', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const gamma = item('linux', 'Linux', 'b1', 'Gamma')
  const checked = new Set(['linux/b1', 'mac/b1'])
  assert.deepEqual(filterChecked([alpha, beta, gamma], checked).map((i) => i.bot.name), ['Alpha', 'Gamma'])
  assert.deepEqual(filterChecked([alpha, beta], new Set()), [])
})

test('Select all checks exactly the roster, because allKeys and filterChecked agree', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha'), item('linux', 'Linux', 'b2', 'Beta')]
  assert.deepEqual(filterChecked(items, new Set(allKeys(items))).map((i) => i.bot.name), ['Alpha', 'Beta'])
})

test('hiddenCheckedCount counts only the checked bots the filter hides', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const gamma = item('linux', 'Linux', 'b1', 'Gamma')
  const checked = new Set(['mac/b1', 'mac/b2', 'linux/b1'])
  // Beta stays visible, so only Alpha and Gamma are hidden by the filter.
  assert.equal(hiddenCheckedCount([alpha, beta, gamma], [beta], checked), 2)
  assert.equal(hiddenCheckedCount([alpha, beta], [alpha, beta], checked), 0)
  assert.equal(hiddenCheckedCount([alpha, beta], [beta], new Set(['mac/b1', 'mac/b2'])), 1)
  // A checked key that left the roster entirely is gone, not hidden.
  assert.equal(hiddenCheckedCount([alpha], [alpha], new Set(['mac/b2'])), 0)
  assert.equal(hiddenCheckedCount([alpha, beta], [alpha, beta], new Set()), 0)
})

test('applyTargets gates Apply to what the filter shows (the F1 regression pin)', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const gamma = item('linux', 'Linux', 'b1', 'Gamma')
  // check A -> search B -> check B: both checked, but only Beta is visible.
  const checked = new Set(['mac/b1', 'mac/b2'])
  const t = applyTargets([alpha, beta, gamma], [beta], checked)
  assert.deepEqual(t.chosen.map((i) => i.bot.name), ['Beta'])
  assert.equal(t.hiddenCount, 1)
  assert.equal(t.totalChecked, 2)
})

test('applyTargets with no filter leaves every checked bot as chosen', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha'), item('linux', 'Linux', 'b2', 'Beta')]
  const t = applyTargets(items, items, new Set(allKeys(items)))
  assert.equal(t.chosen.length, 2)
  assert.equal(t.hiddenCount, 0)
  assert.equal(t.totalChecked, 2)
})

test('applyTargets treats a checked key that left the roster as gone', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const t = applyTargets([alpha], [alpha], new Set(['mac/b9']))
  assert.equal(t.chosen.length, 0)
  assert.equal(t.hiddenCount, 0)
  assert.equal(t.totalChecked, 0)
})

test('toggleKey adds and removes without touching the set it was given', () => {
  const before = new Set(['mac/b1'])
  const added = toggleKey(before, 'linux/b2', true)
  const removed = toggleKey(before, 'mac/b1', false)
  assert.deepEqual([...added].sort(), ['linux/b2', 'mac/b1'])
  assert.deepEqual([...removed], [])
  assert.deepEqual([...before], ['mac/b1'])
  assert.deepEqual([...toggleKey(before, 'mac/b1', true)], ['mac/b1'])
})

test('planRows saves every bot when the agent default is chosen', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha'), item('linux', 'Linux', 'b2', 'Beta')]
  const plan: ApplyPlan = { items, backend: 'claude', model: null, options: {} }
  assert.deepEqual(planRows(plan, () => no).map((r) => r.ok), [true, true])
})

test('planRows saves a model the computer advertises', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha'), item('linux', 'Linux', 'b2', 'Beta')]
  const plan: ApplyPlan = { items, backend: 'claude', model: 'opus', options: { effort: 'high' } }
  const rows = planRows(plan, () => ok(['opus', 'sonnet']))
  assert.deepEqual(rows.map((r) => r.ok), [true, true])
  assert.equal(rows[0]?.reason, undefined)
})

test('planRows skips the computer without the model, and says which computer', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const linux = item('linux', 'Linux Studio', 'b2', 'Beta')
  const plan: ApplyPlan = { items: [mac, linux], backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, (id) => (id === 'mac' ? ok(['opus']) : ok(['sonnet'])))
  assert.deepEqual(rows.map((r) => r.ok), [true, false])
  assert.equal(rows[0]?.reason, undefined)
  assert.match(rows[1]?.reason ?? '', /Linux Studio/)
})

test('planRows refuses a computer whose catalog could not be read', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const plan: ApplyPlan = { items: [mac], backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, () => bad)
  assert.deepEqual(rows.map((r) => r.ok), [false])
  assert.match(rows[0]?.reason ?? '', /Mac/)
})

test('planRows refuses a computer whose catalog has not loaded, rather than saving unverified', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha')]
  const plan: ApplyPlan = { items, backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, () => no)
  assert.deepEqual(rows.map((r) => r.ok), [false])
  assert.match(rows[0]?.reason ?? '', /Mac/)
})

test('planRows never checks a custom command, which has no catalog', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha')]
  const plan: ApplyPlan = { items, backend: 'custom', model: 'my-agent --model opus', options: {} }
  assert.deepEqual(planRows(plan, () => ok(['sonnet'])).map((r) => r.ok), [true])
})

test('the palette search narrows the list, so only matching bots can be checked', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const matched = [alpha, beta].filter((i) => i.bot.name.toLowerCase().includes('alpha'))
  assert.deepEqual(filterChecked(matched, new Set(allKeys([alpha, beta]))).map((i) => i.bot.name), ['Alpha'])
})

test('defaultSheetSize opens on the longest bot name and about three quarters of the height', () => {
  // 40 characters at 8px each is 320 wide; with 460px of chrome around it the sheet wants 780.
  const size = defaultSheetSize(['Short', 'A very long bot name that goes on and on'], (text) => text.length * 8, 1600, 1000)
  assert.deepEqual(size, { width: 780, height: 780 })
})

test('defaultSheetSize never exceeds the main window', () => {
  // A name wider than the screen is capped; on a short window the usable floor (560) applies,
  // still capped under the viewport so the card never overflows.
  const size = defaultSheetSize(['x'.repeat(200)], (text) => text.length * 8, 900, 700)
  assert.deepEqual(size, { width: 860, height: 560 })
})

test('clampSheetSize keeps a drag inside the bounds and the viewport', () => {
  assert.deepEqual(clampSheetSize(300, 300, 1600, 1000), { width: 480, height: 420 })
  assert.deepEqual(clampSheetSize(5000, 5000, 1000, 800), { width: 960, height: 752 })
  assert.deepEqual(clampSheetSize(700.4, 620.6, 1600, 1000), { width: 700, height: 621 })
})
