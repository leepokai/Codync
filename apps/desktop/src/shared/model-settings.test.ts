import { test } from 'node:test'
import assert from 'node:assert/strict'
import { effortRow, effortSetting, modelSettings, reportFor } from './model-settings.ts'

const effort = { id: 'reasoning_effort', name: 'Reasoning Effort', default: 'low', values: [{ id: 'low', name: 'Low' }, { id: 'high', name: 'High' }] }
const catalog = {
  models: [{ id: 'gpt-6.1-sol', options: [effort] }, { id: 'gpt-6.1-mini', options: [] }],
  currentModelId: 'gpt-6.1-sol',
}

test('a bot on the Default model gets the settings of the model the agent runs by default', () => {
  assert.deepEqual(modelSettings(catalog, ''), [effort])
})

test('a chosen model shows its own settings', () => {
  assert.deepEqual(modelSettings(catalog, 'gpt-6.1-mini'), [])
  assert.deepEqual(modelSettings(catalog, 'gpt-6.1-sol'), [effort])
})

test('no catalog, an unlisted model or an unknown default shows no settings', () => {
  assert.deepEqual(modelSettings(null, ''), [])
  assert.deepEqual(modelSettings(catalog, 'typed-by-hand'), [])
  assert.deepEqual(modelSettings({ models: catalog.models }, ''), [])
})

const level = { id: 'effort', name: 'Thinking', category: 'thought_level', default: 'low', values: [{ id: 'low', name: 'Low' }, { id: 'high', name: 'High' }] }
const verbosity = { id: 'verbosity', name: 'Verbosity', category: '_custom', values: [{ id: 'terse', name: 'Terse' }] }

test('the effort level is the thought_level setting, whatever the agent calls it', () => {
  assert.equal(effortSetting([verbosity, level]), level)
  assert.equal(effortSetting([verbosity]), undefined)
  assert.equal(effortSetting([effort]), undefined, 'no category: not claimed as the effort level')
})

const runtime = (over: Record<string, unknown> = {}) => ({
  agentStep: null, sessionLive: true, sessionOptions: { effort: 'low' }, sessionSettings: [level], settingIssues: {}, restartPending: false, ...over,
})

test('a step hears only its own agent: the running one, the bot’s own by default', () => {
  assert.equal(reportFor(runtime(), 0)?.options?.effort, 'low')
  assert.equal(reportFor(runtime(), 1), null, 'a fallback that isn’t running says nothing')
  const onFallback = runtime({ agentStep: { step: 1 } })
  assert.equal(reportFor(onFallback, 0), null, 'the bot’s own agent isn’t running')
  assert.equal(reportFor(onFallback, 1)?.options?.effort, 'low')
})

const caption = (saved: string | undefined, report: ReturnType<typeof reportFor>, over: Record<string, unknown> = {}) =>
  effortRow({ catalogSettings: [level], saved, report, loading: false, failed: false, chosenModel: false, ...over })

test('the caption says what the running session confirms, never what was only saved', () => {
  const r = (over: Record<string, unknown> = {}) => reportFor(runtime(over), 0)
  assert.deepEqual(caption('high', r({ sessionOptions: { effort: 'high' } })).caption, 'In use: High')
  assert.deepEqual(caption(undefined, r()).caption, 'In use: Low (agent default)')
  assert.deepEqual(caption('high', r()).caption, 'Saved: High · not confirmed (in use: Low)')
  assert.deepEqual(caption('high', r({ sessionOptions: {} })).caption, 'Current: unknown')
  assert.deepEqual(caption('high', r({ sessionLive: false, sessionOptions: null })).caption, 'Not running · Saved: High')
  assert.deepEqual(caption(undefined, null).caption, 'Not running')
})

test('a change waiting for the reply shows both values', () => {
  const r = (over: Record<string, unknown>) => reportFor(runtime({ restartPending: true, ...over }), 0)
  assert.equal(caption('high', r({})).caption, 'In use: Low · High pending')
  assert.equal(caption(undefined, r({ sessionOptions: { effort: 'high' } })).caption, 'In use: High · Default pending')
  assert.equal(caption('high', r({ sessionOptions: {} })).caption, 'Current: unknown · High pending')
})

test('what went wrong with the saved value is said for that value only', () => {
  const issue = (kind: string, value: string, options: Record<string, string> | null = { effort: 'low' }) =>
    reportFor(runtime({ settingIssues: { effort: { issue: kind, value } }, sessionOptions: options, sessionLive: options !== null }), 0)
  assert.equal(caption('high', issue('rejected', 'high', null)).caption, 'Saved: High · not applied (the agent refused it)')
  assert.equal(caption('high', issue('not_offered', 'high')).caption, 'In use: Low · Saved: High not offered')
  assert.equal(caption('high', issue('not_offered', 'high', null)).caption, 'Saved: High · not offered for this model')
  assert.equal(caption('low', issue('rejected', 'high')).caption, 'In use: Low', 'an issue about another value is old news')
})

test('the row says when this agent’s options are unknown, and is absent when it has none', () => {
  assert.equal(caption('high', null, { loading: true, catalogSettings: [] }).caption, 'Saved: high · checking this agent’s options')
  assert.equal(caption('high', null, { failed: true, catalogSettings: [] }).caption, 'Saved: high · couldn’t check this agent’s options')
  assert.equal(caption(undefined, null, { failed: true, catalogSettings: [] }).kind, 'none')
  assert.equal(caption(undefined, null, { catalogSettings: [verbosity] }).kind, 'none', 'no effort level: no row')
  assert.equal(caption(undefined, null, { catalogSettings: [], chosenModel: true }).kind, 'later')
  assert.equal(caption('high', null, { catalogSettings: [] }).caption, 'Saved: high · not offered for this model', 'a saved value can still be cleared')
})

test('the running session’s own list decides the settings while nothing is pending', () => {
  const other = { ...level, values: [{ id: 'max', name: 'Max' }] }
  assert.deepEqual(caption('max', reportFor(runtime({ sessionSettings: [other], sessionOptions: { effort: 'max' } }), 0), { catalogSettings: [], chosenModel: true }).setting, other)
  const pending = reportFor(runtime({ sessionSettings: [other], restartPending: true }), 0)
  assert.equal(caption(undefined, pending, { catalogSettings: [], chosenModel: true }).kind, 'later', 'a pending change may be another model')
})
