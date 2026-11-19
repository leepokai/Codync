import assert from 'node:assert/strict'
import { test } from 'node:test'
import { composerEnter } from './composer-keyboard.ts'
const enter = { key: 'Enter', shiftKey: false, altKey: false, repeat: false, isComposing: false, keyCode: 13 }

test('Enter sends; Shift/Option-Enter preserve multiline input', () => {
  assert.equal(composerEnter(enter), 'send')
  assert.equal(composerEnter({ ...enter, shiftKey: true }), 'newline')
  assert.equal(composerEnter({ ...enter, altKey: true }), 'newline')
})

test('CJK candidate confirmation and a held Enter never send', () => {
  assert.equal(composerEnter({ ...enter, isComposing: true }), 'ignore')
  assert.equal(composerEnter({ ...enter, keyCode: 229 }), 'ignore')
  assert.equal(composerEnter({ ...enter, repeat: true }), 'ignore')
})
