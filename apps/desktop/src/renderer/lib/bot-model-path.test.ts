import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '../../shared/models'
import type { BotStore } from '../store/bot-store'
import { botModelPath } from './bot-model-path.ts'

/** A computer's hello, with only the parts a path reads. */
const store = (backends: unknown[] | null) => ({ hello: backends ? { backends } : null }) as unknown as BotStore

const bot = (fields: Record<string, unknown>) => fields as unknown as Bot

test('the path is the agent as hello names it, the login it picked, then the model id', () => {
  const hello = store([
    { id: 'opencode', name: 'OpenCode', loginProfiles: [{ id: 'w', name: 'Work' }] },
  ])
  assert.equal(
    botModelPath(hello, bot({ backend: 'opencode', model: 'opendesign/deepseek-v4.1-flash-eco', loginProfile: 'w' })),
    'OpenCode · Work · opendesign/deepseek-v4.1-flash-eco',
  )
  // The computer's own login (`null`, or absent) names nothing between the two ends.
  assert.equal(
    botModelPath(hello, bot({ backend: 'opencode', model: 'opendesign/hy4-preview', loginProfile: null })),
    'OpenCode · opendesign/hy4-preview',
  )
})

test('an agent hello does not name falls back to the id it runs under', () => {
  // The app's built-in names (`builtInBackendName`) are not this builder's business: the roster's
  // tip says the id, so a bot whose hello is silent still has a path, not the `Custom agent` label.
  assert.equal(botModelPath(store([]), bot({ backend: 'mystery', model: 'some/model' })), 'mystery · some/model')
  assert.equal(botModelPath(store(null), bot({ backend: 'codex', model: 'gpt-5.3-codex' })), 'codex · gpt-5.3-codex')
})

test('an unnamed login profile adds nothing: its id is a uuid, which reads as noise', () => {
  const hello = store([{ id: 'opencode', name: 'OpenCode', loginProfiles: [{ id: '7b3f', name: null }] }])
  assert.equal(
    botModelPath(hello, bot({ backend: 'opencode', model: 'opendesign/hy4-preview', loginProfile: '7b3f' })),
    'OpenCode · opendesign/hy4-preview',
  )
})

test('a custom command, or a bot with no model, has no path at all', () => {
  const hello = store([{ id: 'custom', name: 'Custom agent' }, { id: 'codex', name: 'Codex' }])
  assert.equal(botModelPath(hello, bot({ backend: 'custom', model: 'x' })), null)
  assert.equal(botModelPath(hello, bot({ backend: 'codex', model: null })), null)
  assert.equal(botModelPath(hello, bot({ backend: 'codex', model: '   ' })), null)
})
