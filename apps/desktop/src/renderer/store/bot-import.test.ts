import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { BotStore } from './bot-store'
import { BotImport, type ImportPlan } from './bot-import.ts'

test('a bot and a group sharing a key are picked apart', async () => {
  const sent: unknown[][] = []
  const store = { ready: async () => ({ call: async (method: string, body: unknown) => { sent.push([method, body]); return { bots: [], groups: [], warnings: [] } } }) }
  const model = new BotImport(store as unknown as BotStore)
  const plan = {
    source: 'grok',
    bots: [{ key: 'team', name: 'Team', description: '', skills: [], routines: [], memories: [], files: null }],
    groups: [{ key: 'team', name: 'Team', members: ['team'] }],
    warnings: [],
  } satisfies ImportPlan
  model.step = { kind: 'review', path: '/export.zip', plan, skipped: [] }

  model.toggle('group', 'team')
  assert.equal(model.picked('bot', 'team'), true)
  assert.equal(model.picked('group', 'team'), false)
  await model.importPicked()
  assert.deepEqual(sent, [['importBots', { path: '/export.zip', bots: ['team'], groups: [] }]])

  model.step = { kind: 'review', path: '/export.zip', plan, skipped: [] }
  model.toggle('bot', 'team')
  assert.equal(model.groupAvailable(['team']), false)
  assert.equal(model.canImport, false)
})
