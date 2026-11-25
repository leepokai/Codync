import assert from 'node:assert/strict'
import { test } from 'node:test'
import { anchorFor, rowsCanScroll, scrollTopFor } from './scroll-anchor.ts'

test('a reader at the newest keeps no anchor, so no later render can move them', () => {
  // At the newest the rows are brought to the bottom after every render. An anchor taken there would survive
  // into a later render once the view had left the bottom, and would land above the top of the taller rows.
  assert.equal(anchorFor(true, 3000, 2600), null)
  // A reader in the history keeps theirs: the distance from the rows' end to the view's top.
  assert.equal(anchorFor(false, 3000, 1200), 1800)
})

test('an anchor taken at the newest lands at the very top once the view is no longer following', () => {
  // The bug: 3000 tall, at the bottom (2600), anchor 400. Older rows go in above, the rows become 6000, and the
  // anchor is applied on a later render: 6000 - 400 = 5600 with a 6000-tall list scrolled to its end, so the reader
  // was moved to the bottom, or above the top when the rows grew by more than the anchor.
  assert.equal(scrollTopFor(400, 6000), 5600)
  assert.equal(scrollTopFor(400, 200), -200) // clamped to 0 by the browser: the view sits at its very top
  // With no anchor taken at the newest there is nothing to apply, which is the fix.
  assert.equal(anchorFor(true, 3000, 2600), null)
})

test('a reader in the history keeps their place when older rows arrive above', () => {
  const before = anchorFor(false, 3000, 1200)!
  // 2000 rows arrive above: the rows are now 5000 tall, and the same distance from their end puts the view 2000 lower.
  assert.equal(scrollTopFor(before, 5000), 3200)
  assert.equal(before, 1800)
})

test('rows that cannot scroll are not asked for earlier ones', () => {
  // A short page: the view sits at its top permanently, so "near the top" is always true and says nothing.
  assert.equal(rowsCanScroll(400, 600), false)
  assert.equal(rowsCanScroll(600, 600), false)
  assert.equal(rowsCanScroll(900, 600), true)
})
