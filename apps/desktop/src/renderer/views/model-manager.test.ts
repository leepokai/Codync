import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { RosterItem } from '../store/app-model'
import type { AgentModels } from '@shared/agent-models'
import type { ApplyPlan, CatalogCheck } from './bots/model-manager-logic'
import { allKeys, applyTargets, clampSheetSize, defaultSheetSize, filterChecked, hiddenCheckedCount, pausePatch, planRows, providerOptions, resizeFrom, rowModelPath, stepSize, toggleKey } from './bots/model-manager-logic.ts'

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

test('providerOptions offers every agent the bots run, in hello order, never a hardcoded list', () => {
  const backends = [{ id: 'opencode', name: 'OpenCode' }, { id: 'deepseek', name: 'DeepSeek' }, { id: 'claude', name: 'Claude' }]
  const on = (backend: string, botId: string, name: string): RosterItem =>
    ({ ref: { computerId: 'mac', botId }, bot: { id: botId, name, backend }, computer: { id: 'mac', name: 'Mac' } }) as unknown as RosterItem
  const items = [on('deepseek', 'b1', 'A'), on('opencode', 'b2', 'B'), on('deepseek', 'b3', 'C'), on('mystery', 'b4', 'D')]
  // hello's order, only the agents the bots run, and an agent hello doesn't name by its raw id.
  assert.deepEqual(providerOptions(items, backends), [
    { id: 'opencode', name: 'OpenCode' },
    { id: 'deepseek', name: 'DeepSeek' },
    { id: 'mystery', name: 'mystery' },
  ])
  assert.deepEqual(providerOptions([], backends), [])
  assert.deepEqual(providerOptions([on('claude', 'b1', 'A')], backends), [{ id: 'claude', name: 'Claude' }])
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

test('pausePatch sends only the id and the flag — more minimal than the roster toggle, on the same updateBot path', () => {
  assert.deepEqual(pausePatch('b1', true), { id: 'b1', paused: true })
  assert.deepEqual(pausePatch('b2', false), { id: 'b2', paused: false })
  // Nothing else rides along: the host keeps every other setting the bot has.
  assert.deepEqual(Object.keys(pausePatch('b1', true)).sort(), ['id', 'paused'])
})

test('applyTargets gates a bulk pause to what the filter shows, exactly like Apply', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  // check A -> search B: pausing touches only what the user can see, hidden-checked A stays paused-or-not as it was.
  const t = applyTargets([alpha, beta], [beta], new Set(['mac/b1', 'mac/b2']))
  assert.deepEqual(t.chosen.map((i) => i.bot.name), ['Beta'])
  assert.equal(t.hiddenCount, 1)
  assert.equal(t.totalChecked, 2)
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
  assert.deepEqual(size, { width: 852, height: 560 })
})

test('clampSheetSize keeps a drag inside the bounds and the viewport', () => {
  assert.deepEqual(clampSheetSize(300, 300, 1600, 1000), { width: 480, height: 420 })
  // 48 matches .modal-content's own max-width/max-height, so the state never leads the render.
  assert.deepEqual(clampSheetSize(5000, 5000, 1000, 800), { width: 952, height: 752 })
  assert.deepEqual(clampSheetSize(700.4, 620.6, 1600, 1000), { width: 700, height: 621 })
})

test('resizeFrom anchors to the drag start, so cumulative deltas never compound', () => {
  const anchor = { width: 640, height: 560 }
  const vw = 1600
  const vh = 1000
  // A real drag's move events carry CUMULATIVE deltas: 10, 20, ... 100 from the start. Anchored
  // math lands on 740 exactly; the pre-review bug added each delta to the live size (550 total).
  let size = { ...anchor }
  for (let total = 10; total <= 100; total += 10) size = resizeFrom('right', anchor, total, 0, vw, vh)
  assert.deepEqual(size, { width: 740, height: 560 })
  size = resizeFrom('left', anchor, -30, 0, vw, vh)
  assert.deepEqual(size, { width: 670, height: 560 })
  size = resizeFrom('bottom', anchor, 0, -80, vw, vh)
  assert.deepEqual(size, { width: 640, height: 480 })
  // The bottom handle says height: horizontal drift from a sloppy drag never changes the width.
  size = resizeFrom('bottom', anchor, 40, 60, vw, vh)
  assert.deepEqual(size, { width: 640, height: 620 })
  size = resizeFrom('corner-br', anchor, 40, 20, vw, vh)
  assert.deepEqual(size, { width: 680, height: 580 })
  size = resizeFrom('corner-bl', anchor, -40, 20, vw, vh)
  assert.deepEqual(size, { width: 680, height: 580 })
})

test('stepSize steps only the axis the handle advertises, toward the drag direction', () => {
  const size = { width: 640, height: 560 }
  const vw = 1600
  const vh = 1000
  assert.deepEqual(stepSize('left', size, 'ArrowLeft', vw, vh), { width: 656, height: 560 })
  assert.deepEqual(stepSize('left', size, 'ArrowRight', vw, vh), { width: 624, height: 560 })
  assert.deepEqual(stepSize('right', size, 'ArrowRight', vw, vh), { width: 656, height: 560 })
  // Side handles say width: their Up/Down keys do nothing.
  assert.deepEqual(stepSize('right', size, 'ArrowDown', vw, vh), { width: 640, height: 560 })
  assert.deepEqual(stepSize('left', size, 'ArrowUp', vw, vh), { width: 640, height: 560 })
  // The bottom handle says height: its Left/Right keys do nothing.
  assert.deepEqual(stepSize('bottom', size, 'ArrowDown', vw, vh), { width: 640, height: 576 })
  assert.deepEqual(stepSize('bottom', size, 'ArrowLeft', vw, vh), { width: 640, height: 560 })
  // Corners step both axes toward their own outside.
  assert.deepEqual(stepSize('corner-br', size, 'ArrowRight', vw, vh), { width: 656, height: 560 })
  assert.deepEqual(stepSize('corner-bl', size, 'ArrowLeft', vw, vh), { width: 656, height: 560 })
})

test('rowModelPath shows agent->model, display name when known, nothing when unreadable', () => {
  // The picker knows the agent: its display name; the model shows its id.
  assert.equal(rowModelPath({ backend: 'opencode', model: 'open-design/deepseek 4.1 flash' }, 'OpenCode'), 'OpenCode->open-design/deepseek 4.1 flash')
  // An agent the picker doesn't know falls back to the raw id.
  assert.equal(rowModelPath({ backend: 'mystery', model: 'some/model' }, null), 'mystery->some/model')
  // A blank display name is as good as none.
  assert.equal(rowModelPath({ backend: 'mystery', model: 'some/model' }, '  '), 'mystery->some/model')
  assert.equal(rowModelPath({ backend: 'mystery', model: 'some/model' }, undefined), 'mystery->some/model')
  // Custom-command bots (backend not a string) and unreadable models show nothing.
  assert.equal(rowModelPath({ backend: { command: 'my-bot' }, model: 'x' }, 'OpenCode'), null)
  assert.equal(rowModelPath({ backend: 'custom', model: 'x' }, 'OpenCode'), null)
  assert.equal(rowModelPath({ backend: 'opencode', model: null }, 'OpenCode'), null)
  assert.equal(rowModelPath({ backend: 'opencode', model: '   ' }, 'OpenCode'), null)
  assert.equal(rowModelPath({ backend: 'opencode', model: 5 }, 'OpenCode'), null)
  assert.equal(rowModelPath({ backend: 'opencode' }, 'OpenCode'), null)
  // A padded model id is trimmed into the path.
  assert.equal(rowModelPath({ backend: 'opencode', model: ' some/model ' }, 'OpenCode'), 'OpenCode->some/model')
})
