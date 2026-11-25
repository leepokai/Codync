import assert from 'node:assert/strict'
import { test } from 'node:test'
import { hoverTip, modelPath } from './model-path.ts'

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
  // The manager builds its rows from a host snapshot, where a custom command's backend is still
  // an object and a model can still arrive as anything: no path, never `[object Object]->5`.
  assert.equal(modelPath({ backend: { command: 'my-bot' }, model: 'x' }, 'OpenCode'), null)
  assert.equal(modelPath({ backend: 'opencode', model: 5 }, 'OpenCode'), null)
  assert.equal(modelPath({ backend: 'opencode' }, 'OpenCode'), null)
})

test('the Model Manager row cases name the same string, so the two rows can never drift', () => {
  // These are the manager's own row cases, moved here with its `rowModelPath` (MM sync rule: one
  // implementation, never a copy). If this test ever fails, one of the two rows has drifted.
  assert.equal(modelPath({ backend: 'opencode', model: 'open-design/deepseek 4.1 flash' }, 'OpenCode'), 'OpenCode->open-design/deepseek 4.1 flash')
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
