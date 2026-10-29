import assert from 'node:assert/strict'
import { test } from 'node:test'
import { clockTime, moveFallback, restLine, restOf, stepBanner, withBackend } from './fallbacks.ts'
import { draftOf, normalizeBot } from './models.ts'

const now = new Date(2026, 9, 21, 10, 0).getTime()
const at3 = new Date(2026, 9, 21, 15, 0).getTime()
const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
const names: Record<string, string> = { claude: 'Claude Code', codex: 'Codex' }
const name = (id: string) => names[id] ?? id

test('a time today is just the time; another day names the weekday', () => {
  assert.equal(clockTime(at3, now), time(at3))
  const friday = new Date(2026, 9, 23, 15, 0).getTime()
  assert.equal(clockTime(friday, now), `${new Date(friday).toLocaleDateString(undefined, { weekday: 'short' })} ${time(friday)}`)
})

test('a usage limit says when it resets; other rests say until when', () => {
  assert.equal(restLine({ agent: 'claude', until: at3, reason: 'usageLimit' }, now), `Usage resets ${time(at3)}`)
  assert.equal(restLine({ agent: 'claude', until: at3, reason: 'rateLimited' }, now), `Resting until ${time(at3)}`)
})

test('a rest that ended no longer counts', () => {
  const bot = normalizeBot({ id: 'b', resting: [{ agent: 'claude', until: now - 1, reason: 'usageLimit' }] })
  assert.equal(restOf(bot, 'claude', now), null)
  assert.equal(restOf(bot, 'claude', now - 2)?.reason, 'usageLimit')
})

test('the banner names the fallback, and when the own agent is back if it says', () => {
  const step = { step: 1, agent: 'codex', model: null, since: now, reason: 'usageLimit' }
  assert.equal(stepBanner(normalizeBot({ id: 'b', backend: 'claude' }), name, now), null)
  const resting = normalizeBot({ id: 'b', backend: 'claude', agentStep: step, resting: [{ agent: 'claude', until: at3, reason: 'usageLimit' }] })
  assert.equal(stepBanner(resting, name, now), `Using Codex until Claude Code resets at ${time(at3)}`)
  const failing = normalizeBot({ id: 'b', backend: 'claude', agentStep: step, resting: [{ agent: 'claude', until: now - 1, reason: 'usageLimit' }] })
  assert.equal(stepBanner(failing, name, now), "Using Codex while Claude Code can't answer")
})

test('fallbacks move within the list', () => {
  const f = (backend: string) => ({ backend, model: null, modelOptions: {} })
  const list = [f('codex'), f('gemini'), f('grok')]
  assert.deepEqual(moveFallback(list, 0, 1).map((x) => x.backend), ['gemini', 'codex', 'grok'])
  assert.deepEqual(moveFallback(list, 2, -1).map((x) => x.backend), ['codex', 'grok', 'gemini'])
  assert.deepEqual(moveFallback(list, 0, -1).map((x) => x.backend), ['codex', 'gemini', 'grok'])
  assert.deepEqual(list.map((x) => x.backend), ['codex', 'gemini', 'grok'])
})

test('making a fallback the bot\'s own agent drops it from the list', () => {
  const draft = draftOf(normalizeBot({ id: 'b', backend: 'claude', fallbacks: [{ backend: 'codex', model: 'gpt-5.5', modelOptions: {} }, { backend: 'gemini', model: null, modelOptions: {} }] }))
  const next = withBackend(draft, 'codex')
  assert.equal(next.backend, 'codex')
  assert.deepEqual(next.fallbacks?.map((f) => f.backend), ['gemini'])
})

test('an older host has no fallbacks to show or send', () => {
  const bot = normalizeBot({ id: 'b' })
  assert.equal(bot.selfFallbacks, null)
  assert.equal(draftOf(bot).selfFallbacks, undefined)
  assert.equal(normalizeBot({ id: 'b', selfFallbacks: false }).selfFallbacks, false)
})


test('switching harness clears its model and effort, selecting the same harness preserves them', () => {
  const draft = draftOf(normalizeBot({ id: 'b', backend: 'claude', model: 'opus', modelOptions: { effort: 'high' } }))
  assert.equal(withBackend(draft, 'claude'), draft)
  const changed = withBackend(draft, 'codex')
  assert.equal(changed.model, null)
  assert.deepEqual(changed.modelOptions, {})
  const defaultModel = withBackend({ ...draft, model: null }, 'codex')
  assert.deepEqual(defaultModel.modelOptions, {})
})
