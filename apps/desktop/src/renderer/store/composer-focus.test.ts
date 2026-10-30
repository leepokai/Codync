import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ComposerFocus } from './composer-focus.ts'

const a = { computerId: 'mac', botId: 'a' }
const b = { computerId: 'mac', botId: 'b' }

test('a clicked chat takes the caret once', () => {
  const focus = new ComposerFocus()
  focus.open(a)
  focus.select(a)
  assert.equal(focus.take(b), false)
  assert.equal(focus.take(a), true)
  assert.equal(focus.take(a), false)
})

test('clicking the open chat again brings the caret back', () => {
  const focus = new ComposerFocus()
  focus.open(a)
  focus.take(a)
  focus.open(a)
  focus.select(a)
  assert.equal(focus.take(a), true)
})

test('moving through the roster with the keyboard leaves the caret where it is', () => {
  const focus = new ComposerFocus()
  focus.select(a)
  assert.equal(focus.take(a), false)
  focus.open(a)
  focus.select(b)
  assert.equal(focus.take(b), false)
  assert.equal(focus.take(a), false)
})

test('the same bot on another computer is another chat', () => {
  const focus = new ComposerFocus()
  focus.open(a)
  assert.equal(focus.take({ computerId: 'linux', botId: 'a' }), false)
})
