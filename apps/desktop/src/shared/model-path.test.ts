import assert from 'node:assert/strict'
import { test } from 'node:test'
import { hoverTip, loginNameOf, modelPath } from './model-path.ts'

test('a bot with an agent and a model is named agent->model', () => {
  assert.equal(modelPath({ backend: 'codex', model: 'gpt-5.3-codex' }, 'Codex'), 'Codex->gpt-5.3-codex')
})

test('the agent falls back to its id when the roster has not named it yet', () => {
  assert.equal(modelPath({ backend: 'codex', model: 'gpt-5.3-codex' }, null), 'codex->gpt-5.3-codex')
  assert.equal(modelPath({ backend: 'codex', model: 'gpt-5.3-codex' }, '  '), 'codex->gpt-5.3-codex')
})

test('a custom command has no model path of its own', () => {
  assert.equal(modelPath({ backend: 'custom', model: 'anything' }, 'Custom'), null)
})

test('a bot with no model has no path, so the tip keeps one line', () => {
  assert.equal(modelPath({ backend: 'codex', model: null }, 'Codex'), null)
  assert.equal(modelPath({ backend: 'codex', model: '' }, 'Codex'), null)
  assert.equal(modelPath({ backend: 'codex', model: '   ' }, 'Codex'), null)
})

test('a model name is trimmed', () => {
  assert.equal(modelPath({ backend: 'codex', model: '  gpt-5.3-codex  ' }, 'Codex'), 'Codex->gpt-5.3-codex')
})

test('a row the host has not narrowed yet names nothing rather than guessing', () => {
  // A row can arrive from a host snapshot, where a custom command's backend is still an object and
  // a model can still arrive as anything: no path, never `[object Object]->5`.
  assert.equal(modelPath({ backend: { command: 'my-bot' }, model: 'x' }, 'OpenCode'), null)
  assert.equal(modelPath({ backend: 'opencode', model: 5 }, 'OpenCode'), null)
  assert.equal(modelPath({ backend: 'opencode' }, 'OpenCode'), null)
})

test('a login profile the bot picks sits between its agent and its model', () => {
  assert.equal(
    modelPath({ backend: 'opencode', model: 'opendesign/deepseek-v4.1-flash-eco' }, 'OpenCode', 'Work'),
    'OpenCode->Work->opendesign/deepseek-v4.1-flash-eco',
  )
  // The computer's own login has no name to add, so blank and absent read the same.
  assert.equal(modelPath({ backend: 'opencode', model: 'opendesign/hy4-preview' }, 'OpenCode', '  '), 'OpenCode->opendesign/hy4-preview')
  assert.equal(modelPath({ backend: 'opencode', model: 'opendesign/hy4-preview' }, 'OpenCode', null), 'OpenCode->opendesign/hy4-preview')
  // A login name is trimmed like the agent's.
  assert.equal(modelPath({ backend: 'opencode', model: 'some/model' }, 'OpenCode', '  Work  '), 'OpenCode->Work->some/model')
})

test('loginNameOf names the profile the computer named, and nothing for the default or an unnamed one', () => {
  const backends = [{ id: 'opencode', loginProfiles: [{ id: 'w', name: 'Work' }, { id: '7b3f', name: null }] }]
  assert.equal(loginNameOf(backends, 'opencode', 'w'), 'Work')
  // An unnamed profile's id is a uuid: it would read as noise, so it is left out.
  assert.equal(loginNameOf(backends, 'opencode', '7b3f'), null)
  // null (and absent) is the computer's own login, which has no name to add.
  assert.equal(loginNameOf(backends, 'opencode', null), null)
  assert.equal(loginNameOf(backends, 'opencode', ''), null)
  // An agent hello doesn't know, or one with no login profiles: nothing.
  assert.equal(loginNameOf(backends, 'mystery', 'w'), null)
  assert.equal(loginNameOf([{ id: 'grok' }], 'grok', 'w'), null)
})

test('the Model Manager row cases are pinned here, login included', () => {
  // Copied from the manager's own `rowModelPath` cases: the roster must print what the manager
  // prints, so when the manager changes a case this one changes with it.
  assert.equal(modelPath({ backend: 'opencode', model: 'open-design/deepseek 4.1 flash' }, 'OpenCode'), 'OpenCode->open-design/deepseek 4.1 flash')
  assert.equal(modelPath({ backend: 'opencode', model: 'opendesign/deepseek-v4.1-flash-eco' }, 'OpenCode', 'Work'), 'OpenCode->Work->opendesign/deepseek-v4.1-flash-eco')
  assert.equal(modelPath({ backend: 'mystery', model: 'some/model' }, null), 'mystery->some/model')
  assert.equal(modelPath({ backend: 'mystery', model: 'some/model' }, undefined), 'mystery->some/model')
  assert.equal(modelPath({ backend: 'opencode', model: ' some/model ' }, 'OpenCode'), 'OpenCode->some/model')
  assert.equal(modelPath({ backend: 'opencode', model: '   ' }, 'OpenCode'), null)
  assert.equal(modelPath({ backend: 'opencode', model: null }, 'OpenCode'), null)
})

test('blank lines never reach the tip', () => {
  assert.equal(hoverTip(['Bot', '  ']), 'Bot')
  assert.equal(hoverTip([]), '')
})

test('the tip is the heading on one line and the model on the next', () => {
  assert.equal(hoverTip(['Rusty', 'Codex->gpt-5.3-codex']), 'Rusty\nCodex->gpt-5.3-codex')
})

test('a bot without a model keeps a one-line tip — no blank second line', () => {
  assert.equal(hoverTip(['Rusty', null]), 'Rusty')
  assert.equal(hoverTip(['Rusty', undefined]), 'Rusty')
  assert.equal(hoverTip(['Rusty', '']), 'Rusty')
})

test('the computer joins the heading when the roster spans more than one', () => {
  assert.equal(hoverTip(['Rusty · RYANMANOVERKILL', 'Codex->gpt-5.3-codex']), 'Rusty · RYANMANOVERKILL\nCodex->gpt-5.3-codex')
})
