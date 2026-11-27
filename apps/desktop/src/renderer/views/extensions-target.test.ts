import assert from 'node:assert/strict'
import { test } from 'node:test'
import { extensionsTarget } from './extensions-target.ts'

test('a computer asked for by name leads, so a heading or deep link keeps its own', () => {
  assert.equal(extensionsTarget('laptop', 'desk', 'desk', 'desk'), 'laptop')
})

test('with no computer asked for, the selected bot\'s computer leads', () => {
  assert.equal(extensionsTarget(null, 'laptop', 'desk', 'desk'), 'laptop')
})

test('with nothing asked for or selected, this computer shows, then any online one', () => {
  assert.equal(extensionsTarget(null, null, 'desk', 'other'), 'desk')
  assert.equal(extensionsTarget(null, null, null, 'other'), 'other')
  assert.equal(extensionsTarget(undefined, undefined, undefined, undefined), null)
})

test('clearing what the rail asked for hands Extensions back to the selection', () => {
  // A rail click sets no computer (null), so the bot selected now decides where Extensions opens.
  assert.equal(extensionsTarget(null, 'computer-b', 'computer-a', 'computer-a'), 'computer-b')
})
