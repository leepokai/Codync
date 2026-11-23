import { test } from 'node:test'
import assert from 'node:assert/strict'
import type { RosterItem } from '../store/app-model'
import type { AgentModels } from './bots/agent-models'
import type { ApplyPlan } from './bots/model-manager-logic'
import { allKeys, filterChecked, planRows, toggleKey } from './bots/model-manager-logic.ts'

/** A roster row carries a store, a bot and a computer; these tests only read ref, bot and computer. */
const item = (computerId: string, computerName: string, botId: string, name: string, description = ''): RosterItem =>
  ({
    ref: { computerId, botId },
    bot: { id: botId, name, description, lastMessage: null },
    computer: { id: computerId, name: computerName },
    store: { computer: { id: computerId } },
  }) as unknown as RosterItem

const catalog = (...ids: string[]): AgentModels => ({ models: ids.map((id) => ({ id, name: id })) })

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
  assert.deepEqual(planRows(plan, () => null).map((r) => r.ok), [true, true])
})

test('planRows saves a model the computer advertises', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha'), item('linux', 'Linux', 'b2', 'Beta')]
  const plan: ApplyPlan = { items, backend: 'claude', model: 'opus', options: { effort: 'high' } }
  const rows = planRows(plan, () => catalog('opus', 'sonnet'))
  assert.deepEqual(rows.map((r) => r.ok), [true, true])
  assert.equal(rows[0]?.reason, undefined)
})

test('planRows skips the computer without the model, and says which computer', () => {
  const mac = item('mac', 'Mac', 'b1', 'Alpha')
  const linux = item('linux', 'Linux Studio', 'b2', 'Beta')
  const plan: ApplyPlan = { items: [mac, linux], backend: 'claude', model: 'opus', options: {} }
  const rows = planRows(plan, (id) => (id === 'mac' ? catalog('opus') : catalog('sonnet')))
  assert.deepEqual(rows.map((r) => r.ok), [true, false])
  assert.equal(rows[0]?.reason, undefined)
  assert.match(rows[1]?.reason ?? '', /Linux Studio/)
})

test('planRows leaves a computer whose catalog has not loaded to the host', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha')]
  const plan: ApplyPlan = { items, backend: 'claude', model: 'opus', options: {} }
  assert.deepEqual(planRows(plan, () => null).map((r) => r.ok), [true])
})

test('planRows never checks a custom command, which has no catalog', () => {
  const items = [item('mac', 'Mac', 'b1', 'Alpha')]
  const plan: ApplyPlan = { items, backend: 'custom', model: 'my-agent --model opus', options: {} }
  assert.deepEqual(planRows(plan, () => catalog('sonnet')).map((r) => r.ok), [true])
})

test('the palette search narrows the list, so only matching bots can be checked', () => {
  const alpha = item('mac', 'Mac', 'b1', 'Alpha')
  const beta = item('mac', 'Mac', 'b2', 'Beta')
  const matched = [alpha, beta].filter((i) => i.bot.name.toLowerCase().includes('alpha'))
  assert.deepEqual(filterChecked(matched, new Set(allKeys([alpha, beta]))).map((i) => i.bot.name), ['Alpha'])
})
