import assert from 'node:assert/strict'
import { test } from 'node:test'
import { keptPlace, placeApplies, placeOf, rowsCanScroll, shiftTo, type KeptPlace } from './scroll-anchor.ts'

/** Rows 60 px tall, the first starting `top` px below the view's top (negative: above it). */
const rows = (count: number, top = 0) => Array.from({ length: count }, (_, i) => ({ id: `r${i}`, top: top + i * 60 }))

test('the row straddling the top edge of the view is kept, with its start above that edge', () => {
  // Scrolled 150 px in: r0 is 150 above the top edge, r1 90 above it, r2 starts 30 px above it and runs past it. r2
  // is the row the reader is reading, so r2 is what is kept.
  assert.deepEqual(placeOf(rows(12, -150)), { id: 'r2', offset: -30 })
  // Scrolled to exactly a row's start: that row is below the edge, so the row over it is the one kept.
  assert.deepEqual(placeOf(rows(12, -180)), { id: 'r2', offset: -60 })
  // Scrolled into the rows' top margin: no row has reached the top edge, so the first row is kept where it sits.
  assert.deepEqual(placeOf(rows(12, 500)), { id: 'r0', offset: 500 })
  assert.equal(placeOf([]), null)
})

test('a page of earlier rows moves the view down by exactly how far the kept row moved', () => {
  const place = placeOf(rows(20, -150))!
  // 2400 px of earlier rows arrived above it: the kept row now starts 2400 px lower than the view's top.
  assert.equal(shiftTo(place, -30 + 2400), 2400)
  // Nothing above it moved — a picture loading below, the message box growing — so the view stays put.
  assert.equal(shiftTo(place, -30), 0)
})

test('a place belongs only to the render that shows the rows from an earlier first row', () => {
  const kept: KeptPlace = { place: { id: 'r2', offset: -30 }, start: 'r0' }
  // A message arriving, the working line changing, the message box growing: the same first row, so it waits.
  assert.equal(placeApplies(kept, 'r0', false), false)
  // The earlier page arrives: the rows now start 40 rows earlier, which is the render the place was kept for.
  assert.equal(placeApplies(kept, 'r-40', false), true)
  // A reader who went back to the newest keeps none: there the rows are brought to the bottom after every render.
  assert.equal(placeApplies(kept, 'r-40', true), false)
  assert.equal(placeApplies(null, 'r-40', false), false)
  // And a reader at the newest has no place to keep in the first place.
  assert.equal(keptPlace(rows(20, -150), 'r0', true), null)
})

test('a place kept for a page that never arrives cannot reach the top', () => {
  // What the old distance did: kept at the bottom of 3000 px of rows, then applied once the rows had outgrown it, as
  // an absolute scrollTop the browser clamped to 0 — and every render after clamped it there again. The row-anchored
  // place moves the view by a delta, and that delta is 0 when its row did not move.
  const place = placeOf(rows(20, -150))!
  assert.equal(shiftTo(place, -30), 0)
  // And when its row did move, the view follows it down by as much rather than to an absolute place of its own.
  assert.equal(shiftTo(place, 5000 - 30), 5000)
})

test('rows that cannot scroll are not asked for earlier ones', () => {
  // A short page: the view sits at its top permanently, so "near the top" is always true and says nothing.
  assert.equal(rowsCanScroll(400, 600), false)
  assert.equal(rowsCanScroll(600, 600), false)
  assert.equal(rowsCanScroll(900, 600), true)
})
