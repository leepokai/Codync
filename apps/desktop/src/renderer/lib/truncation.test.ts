import assert from 'node:assert/strict'
import { test } from 'node:test'
import { isTruncated } from './truncation.ts'

test('a box wider than its text is not clipping anything', () => {
  assert.equal(isTruncated(120, 300), false)
  assert.equal(isTruncated(300, 300), false)
})

test('a box narrower than its text is clipping the end of it', () => {
  // The reader sees an ellipsis; the full line is `scrollWidth` wide.
  assert.equal(isTruncated(520, 300), true)
})

test('a box one pixel over is rounding, not clipping', () => {
  // A rounded layout can put scrollWidth a fraction over clientWidth on a line that fits.
  assert.equal(isTruncated(301, 300), false)
  assert.equal(isTruncated(302, 300), true)
})
