import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { RosterItem } from '../store/app-model'
import type { AgentModels } from '@shared/agent-models'
import type { ApplyPlan, CatalogCheck, PlanRow } from './bots/model-manager-logic'
import { agentNameOf, allKeys, applyPatch, applyTargets, catalogRequest, clampSheetSize, computerBackends, countApplied, countedPause, countFailure, defaultSheetSize, filterChecked, hiddenCheckedCount, loginNameOf, loginProfileFor, modelPathWords, needsCatalog, pausePatch, planRows, providerOptions, remainingTargets, resizeFrom, rowModelPath, runTally, stepSize, toggleKey } from './bots/model-manager-logic.ts'

/** A roster row carries a store, a bot and a computer; these tests only read ref, bot and computer. */
const item = (computerId: string, computerName: string, botId: string, name: string, description = '', loginProfile: string | null = null): RosterItem =>
  ({
    ref: { computerId, botId },
    bot: { id: botId, name, description, loginProfile, lastMessage: null },
    computer: { id: computerId, name: computerName },
    store: { computer: { id: computerId } },
  }) as unknown as RosterItem

/** A refused row, asserted: a test that expects a reason fails on a save instead of reading `undefined`. */
const refusalOf = (row: PlanRow | undefined, what = 'the row'): Extract<PlanRow, { ok: false }> => {
  assert.ok(row, `${what} is there`)
  assert.ok(!row.ok, `${what} is a refusal, not a save`)
  return row
}

/** A row nobody refused: the other arm, asserted the same way. */
const assertSaved = (row: PlanRow | undefined, what = 'the row') => {
  assert.ok(row, `${what} is there`)
  assert.ok(row.ok, `${what} is a save, not a refusal`)
}

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
  const rows = planRows(plan, () => effortCatalog('opus', ['low', 'high']))
  assert.deepEqual(rows.map((r) => r.ok), [true, true])
  assertSaved(rows[0])
})

test('planRows skips the computer without the model, and says which computer', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const linux = item('linux', 'Linux Studio', 'b2', 'Beta')
  const plan: ApplyPlan = { items: [mac, linux], backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, (row) => (row.ref.computerId === 'mac' ? ok(['opus']) : ok(['sonnet'])))
  assert.deepEqual(rows.map((r) => r.ok), [true, false])
  assertSaved(rows[0])
  assert.match(refusalOf(rows[1]).reason, /Linux Studio/)
})

test('planRows refuses a computer whose catalog could not be read', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const plan: ApplyPlan = { items: [mac], backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, () => bad)
  assert.deepEqual(rows.map((r) => r.ok), [false])
  assert.match(refusalOf(rows[0]).reason, /Mac/)
})

test('planRows refuses a computer whose catalog has not loaded, rather than saving unverified', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha')]
  const plan: ApplyPlan = { items, backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, () => no)
  assert.deepEqual(rows.map((r) => r.ok), [false])
  assert.match(refusalOf(rows[0]).reason, /Mac/)
})

test('planRows never checks a custom command, which has no catalog', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha')]
  const plan: ApplyPlan = { items, backend: 'custom', model: 'my-agent --model opus', options: {} }
  assert.deepEqual(planRows(plan, () => ok(['sonnet'])).map((r) => r.ok), [true])
})

test('planRows checks each bot on its own login profile, even on one computer', () => {
  const work = item('mac', 'Mac', 'b1', 'Alpha', '', 'work')
  const home = item('mac', 'Mac', 'b2', 'Beta', '', 'home')
  const plan: ApplyPlan = { items: [work, home], backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, (row) => (row.bot.loginProfile === 'work' ? ok(['opus']) : ok(['sonnet'])))
  assert.deepEqual(rows.map((r) => r.ok), [true, false])
  assert.match(refusalOf(rows[1]).reason, /Mac/)
})

/** A catalog whose one model lists `effort` as exactly `values`. */
const effortCatalog = (modelId: string, values: string[], current = modelId): CatalogCheck => ({
  kind: 'ok',
  catalog: {
    currentModelId: current,
    models: [{
      id: modelId,
      name: modelId,
      options: [{ id: 'effort', name: 'Effort', values: values.map((id) => ({ id, name: id })) }],
    }],
  },
})

test('needsCatalog reads a catalog for a named model and for settings on the default', () => {
  assert.equal(needsCatalog('claude', 'opus', {}), true)
  assert.equal(needsCatalog('claude', null, { effort: 'high' }), true)
  assert.equal(needsCatalog('claude', null, {}), false)
  assert.equal(needsCatalog('custom', 'opus', { effort: 'high' }), false)
})

test("the Apply catalog check asks for a new read, not the list the computer kept", () => {
  // A kept list can be ten minutes old: it would refuse a model the agent now offers, or pass one it dropped.
  assert.deepEqual(catalogRequest('claude', null), { backend: 'claude', refresh: true })
  assert.deepEqual(catalogRequest('claude', 'p-1'), { backend: 'claude', loginProfile: 'p-1', refresh: true })
})

test('planRows refuses settings a computer does not list for that model', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const linux = item('linux', 'Linux Studio', 'b2', 'Beta')
  const plan: ApplyPlan = { items: [mac, linux], backend: 'claude', model: 'opus', options: { effort: 'high' } }
  const rows = planRows(plan, (row) => (row.ref.computerId === 'mac' ? effortCatalog('opus', ['low', 'high']) : effortCatalog('opus', ['low'])))
  assert.deepEqual(rows.map((r) => r.ok), [true, false])
  assertSaved(rows[0])
  assert.match(refusalOf(rows[1]).reason, /Those settings aren't available on Linux Studio/)
})

test('planRows checks settings sent with the agent default, and refuses them when the catalog was not loaded', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const linux = item('linux', 'Linux Studio', 'b2', 'Beta')
  const plan: ApplyPlan = { items: [mac, linux], backend: 'claude', model: null, options: { effort: 'high' } }
  const rows = planRows(plan, (row) => (
    row.ref.computerId === 'mac' ? effortCatalog('opus', ['high'], 'opus') : effortCatalog('opus', ['low'], 'opus')
  ))
  assert.deepEqual(rows.map((r) => r.ok), [true, false])
  assert.match(refusalOf(rows[1]).reason, /Those settings aren't available on Linux Studio/)
  const unread = planRows({ ...plan, items: [mac] }, () => no)
  assert.deepEqual(unread.map((r) => r.ok), [false])
  assert.match(refusalOf(unread[0]).reason, /Couldn't check Mac's models/)
})

test('loginProfileFor keeps a bot\'s login only while its agent stays the same', () => {
  assert.equal(loginProfileFor({ backend: 'claude', loginProfile: 'work' }, 'claude'), 'work')
  assert.equal(loginProfileFor({ backend: 'claude', loginProfile: null }, 'claude'), null)
  assert.equal(loginProfileFor({ backend: 'claude' }, 'claude'), null)
  assert.equal(loginProfileFor({ backend: 'claude', loginProfile: 'work' }, 'codex'), null, 'a profile belongs to one agent')
  assert.equal(loginProfileFor({ backend: 'claude', loginProfile: '' }, 'claude'), null)
})

test('applyPatch separately pins the login snapshot the host must not replace', () => {
  const original = item('mac', 'Mac', 'b1', 'Alpha', '', 'work')
  original.bot.backend = 'claude'
  const plan: ApplyPlan = { items: [original], backend: 'codex', model: 'gpt', options: { effort: 'high' } }
  assert.equal(applyPatch(original, plan).expectedLoginProfile, 'work', 'the catalog-checked snapshot is the expectation')
  assert.equal(applyPatch(original, plan).loginProfile, null, 'moving agents uses the default login')
  assert.equal(applyPatch(original, { ...plan, backend: 'claude' }).loginProfile, 'work', 'staying uses the same login')
  assert.equal(applyPatch(original, plan).model, 'gpt')
  assert.deepEqual(applyPatch(original, plan).modelOptions, { effort: 'high' })
  assert.equal(applyPatch(original, plan).command, null)
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
  // A diagonal drag on the bottom edge still carries dx. Width stays put.
  size = resizeFrom('bottom', anchor, 40, -80, vw, vh)
  assert.deepEqual(size, { width: 640, height: 480 })
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
  // The bottom handle says height: its Left/Right keys do nothing.
  assert.deepEqual(stepSize('bottom', size, 'ArrowDown', vw, vh), { width: 640, height: 576 })
  assert.deepEqual(stepSize('bottom', size, 'ArrowLeft', vw, vh), { width: 640, height: 560 })
  // Corners step both axes toward their own outside.
  assert.deepEqual(stepSize('corner-br', size, 'ArrowRight', vw, vh), { width: 656, height: 560 })
  assert.deepEqual(stepSize('corner-bl', size, 'ArrowLeft', vw, vh), { width: 656, height: 560 })
})

test('providerOptions offers every agent the bots run, in hello order, never a hardcoded list', () => {
  const backends = [{ id: 'opencode', name: 'OpenCode' }, { id: 'deepseek', name: 'DeepSeek' }, { id: 'claude', name: 'Claude' }]
  const on = (backend: string, botId: string, name: string): RosterItem =>
    ({ ref: { computerId: 'mac', botId }, bot: { id: botId, name, backend }, computer: { id: 'mac', name: 'Mac' } }) as unknown as RosterItem
  const items = [on('deepseek', 'b1', 'A'), on('opencode', 'b2', 'B'), on('deepseek', 'b3', 'C'), on('mystery', 'b4', 'D')]
  assert.deepEqual(providerOptions(items, backends), [
    { id: 'opencode', name: 'OpenCode' },
    { id: 'deepseek', name: 'DeepSeek' },
    { id: 'mystery', name: 'mystery' },
  ])
  assert.deepEqual(providerOptions([], backends), [])
  assert.deepEqual(providerOptions([on('claude', 'b1', 'A')], backends), [{ id: 'claude', name: 'Claude' }])
})

test('pausePatch sends only the id and the flag', () => {
  assert.deepEqual(pausePatch('b1', true), { id: 'b1', paused: true })
  assert.deepEqual(pausePatch('b2', false), { id: 'b2', paused: false })
  assert.deepEqual(Object.keys(pausePatch('b1', true)).sort(), ['id', 'paused'])
})

test('applyTargets gates a bulk pause to what the filter shows, exactly like Apply', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const t = applyTargets([alpha, beta], [beta], new Set(['mac/b1', 'mac/b2']))
  assert.deepEqual(t.chosen.map((i) => i.bot.name), ['Beta'])
  assert.equal(t.hiddenCount, 1)
  assert.equal(t.totalChecked, 2)
})

test('a bot hidden after the batch started is left out of the remaining writes', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const captured = [alpha, beta]
  const shownAtStart = new Set(['mac/b1', 'mac/b2'])
  const shownAfter = new Set(['mac/b2'])
  assert.deepEqual(remainingTargets(captured, shownAtStart).map((i) => i.bot.name), ['Alpha', 'Beta'])
  assert.deepEqual(remainingTargets(captured, shownAfter).map((i) => i.bot.name), ['Beta'])
  assert.deepEqual(remainingTargets(captured, new Set()).map((i) => i.bot.name), [])
})

test('a successful pause still counts when the sheet has closed', () => {
  assert.deepEqual(countedPause(true, false), { tally: 'applied', show: false })
  assert.deepEqual(countedPause(false, false), { tally: 'failed', show: false })
  assert.deepEqual(countedPause(true, true), { tally: 'applied', show: true })
  assert.deepEqual(countedPause(false, true), { tally: 'failed', show: true })
})

test('rowModelPath shows agent->model, display name when known, nothing when unreadable', () => {
  // The picker knows the agent: its display name; the model shows its id.
  assert.equal(rowModelPath({ backend: 'opencode', model: 'open-design/deepseek 4.1 flash' }, 'OpenCode'), 'OpenCode->open-design/deepseek 4.1 flash')
  // A bot on a login profile names it between the agent and the model. The separator stays `->`.
  assert.equal(rowModelPath({ backend: 'opencode', model: 'opendesign/deepseek-v4.1-flash-eco' }, 'OpenCode', 'Work'), 'OpenCode->Work->opendesign/deepseek-v4.1-flash-eco')
  assert.equal(rowModelPath({ backend: 'opencode', model: 'opendesign/hy4-preview' }, 'OpenCode', '  '), 'OpenCode->opendesign/hy4-preview')
  assert.equal(rowModelPath({ backend: 'opencode', model: 'opendesign/hy4-preview' }, 'OpenCode', null), 'OpenCode->opendesign/hy4-preview')
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

test('agentNameOf names an agent hello knows and answers null for one it does not', () => {
  const backends = [{ id: 'opencode', name: 'OpenCode' }, { id: 'grok', name: 'Grok' }]
  assert.equal(agentNameOf(backends, 'opencode'), 'OpenCode')
  assert.equal(agentNameOf(backends, 'mystery'), null)
  assert.equal(agentNameOf(backends, null), null)
  assert.equal(agentNameOf(backends, { command: 'x' }), null)
})

test('loginNameOf names the login profile a bot picked, and nothing for the computer\'s own login', () => {
  const backends: ReadonlyArray<{ id: string; name: string; loginProfiles?: { id: string; name: string | null }[] }> = [{
    id: 'opencode',
    name: 'OpenCode',
    loginProfiles: [{ id: 'w', name: 'Work' }, { id: '7b3f', name: null }],
  }]
  assert.equal(loginNameOf(backends, 'opencode', 'w'), 'Work')
  assert.equal(loginNameOf(backends, 'opencode', null), null)
  assert.equal(loginNameOf(backends, 'opencode', undefined), null)
  assert.equal(loginNameOf(backends, 'opencode', ''), null)
  assert.equal(loginNameOf(backends, 'opencode', '7b3f'), null)
  assert.equal(loginNameOf(backends, 'mystery', 'w'), null)
  assert.equal(loginNameOf([{ id: 'grok' }], 'grok', 'w'), null)
})

test('a row names the login on its own computer, not the first computer', () => {
  const work = { store: { hello: { backends: [{ id: 'opencode', name: 'OpenCode', loginProfiles: [{ id: 'w', name: 'Work' }] }] } } }
  const home = { store: { hello: { backends: [{ id: 'opencode', name: 'OpenCode', loginProfiles: [{ id: 'w', name: 'Home' }] }] } } }
  assert.equal(loginNameOf(computerBackends(work), 'opencode', 'w'), 'Work')
  assert.equal(loginNameOf(computerBackends(home), 'opencode', 'w'), 'Home')
  assert.notEqual(loginNameOf(computerBackends(home), 'opencode', 'w'), 'Work')
})

test('modelPathWords finds a bot by its agent, provider and model, however the model is typed', () => {
  const words = modelPathWords({ backend: 'opencode', model: 'opendesign/deepseek-v4.1-flash-eco' }, 'OpenCode', 'Work')
  assert.deepEqual(words, [
    'OpenCode->Work->opendesign/deepseek-v4.1-flash-eco',
    'OpenCode',
    'Work',
    'opendesign/deepseek-v4.1-flash-eco',
    'opendesign',
    'deepseek-v4.1-flash-eco',
    'opendesign deepseek v4.1 flash eco',
    'deepseek v4.1 flash eco',
  ])
  assert.deepEqual(modelPathWords({ backend: 'custom', model: 'x' }, 'OpenCode'), [])
  assert.deepEqual(modelPathWords({ backend: 'opencode', model: '  ' }, 'OpenCode'), [])
  const spaced = modelPathWords({ backend: 'opencode', model: 'opendesign/hy4-preview' }, 'OpenCode', 'Work->Home')
  assert.deepEqual(spaced, [
    'OpenCode->Work->Home->opendesign/hy4-preview',
    'OpenCode',
    'Work',
    'Home',
    'opendesign/hy4-preview',
    'opendesign',
    'hy4-preview',
    'opendesign hy4 preview',
    'hy4 preview',
  ])
  assert.deepEqual(modelPathWords({ backend: 'claude', model: 'opus-5.2' }, 'Claude'), [
    'Claude->opus-5.2',
    'Claude',
    'opus-5.2',
    'opus-5.2',
    'opus 5.2',
  ])
})

test('planRows names the kind each refusal counts as, never the reason text', () => {
  const plan: ApplyPlan = { items: [item('mac', 'Mac', 'b1', 'Alpha')], backend: 'claude', model: 'ghost', options: {} }
  const missing = planRows(plan, () => ok(['opus']))
  assert.equal(missing[0]?.ok, false)
  assert.equal(refusalOf(missing[0]).refusal, 'modelMissing')

  const unreadable = planRows(plan, () => bad)
  assert.equal(refusalOf(unreadable[0]).refusal, 'catalogUnreadable')

  const withSettings: ApplyPlan = { ...plan, model: 'opus', options: { effort: 'high' } }
  const refused = planRows(withSettings, () => effortCatalog('opus', ['low']))
  assert.equal(refusalOf(refused[0]).refusal, 'settingsRejected')
})

test('an apply run counts applied once and every failure once in failed and in its kind', () => {
  const tally = runTally(7)
  countApplied(tally)
  countApplied(tally)
  for (const failure of ['modelMissing', 'catalogUnreadable', 'settingsRejected', 'hostGuard', 'saveError'] as const)
    countFailure(tally, failure)

  assert.equal(tally.requested, 7)
  assert.equal(tally.applied, 2)
  assert.equal(tally.failed, 5, 'each failure is counted in failed exactly once')
  const kinds = tally.modelMissing + tally.catalogUnreadable + tally.settingsRejected + tally.hostGuard + tally.saveError
  assert.equal(kinds, tally.failed, 'the five kinds sum to failed')
  const { requested: _r, applied: _a, failed: _f, ...counts } = tally
  assert.deepEqual(Object.values(counts).sort(), [1, 1, 1, 1, 1], 'no kind is counted twice or left out')
})
