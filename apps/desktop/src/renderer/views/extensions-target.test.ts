import assert from 'node:assert/strict'
import { test } from 'node:test'
import { extensionsTarget, railExtensionsOf } from './extensions-target.ts'

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

test('clicking Extensions in the rail drops the computer a heading or deep link asked for', () => {
  // A heading picked computer-a earlier; the user now selected a bot on computer-b and clicked the rail entry.
  const asked = 'computer-a'
  const afterClick = railExtensionsOf(asked, 'extensions')
  assert.equal(afterClick, null, 'the click clears the earlier explicit computer')
  assert.equal(extensionsTarget(afterClick, 'computer-b', 'computer-a', 'computer-a'), 'computer-b', 'and the selection decides')
})

test('a click on another rail entry leaves what Extensions asked for alone', () => {
  assert.equal(railExtensionsOf('computer-a', 'bots'), 'computer-a')
  assert.equal(railExtensionsOf(null, 'bots'), null)
})
