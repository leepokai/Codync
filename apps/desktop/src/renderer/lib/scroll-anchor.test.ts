import assert from 'node:assert/strict'
import { test } from 'node:test'
import { contentTopOf, keptPlace, placeOf, rowsCanScroll, settledPlace, shiftTo } from './scroll-anchor.ts'

/**
 * Rows 60 px tall: the first starts `viewTop` px below the view's top edge (negative: above it) and `scrolled` px
 * into the content, so a row's start in the content is `viewTop + i * 60 + scrolled`.
 */
const rows = (count: number, viewTop = 0, scrolled = 1000) => Array.from({ length: count }, (_, i) => ({
  id: `r${i}`,
  viewTop: viewTop + i * 60,
  contentTop: viewTop + i * 60 + scrolled,
}))

test('the row straddling the top edge of the view is kept, with its start in the content', () => {
  // r2 starts 30 px above the edge and runs past it: r2 is the row the reader is reading. Its start in the content is
  // where it is among the rows, not where the view happens to be scrolled to.
  assert.deepEqual(placeOf(rows(12, -150, 150)), { id: 'r2', top: 120 })
  assert.deepEqual(placeOf(rows(12, -150, 4000)), { id: 'r2', top: 3970 })
  // Scrolled to exactly a row's start (r2 starts 60 px above the edge): that row is above the edge, so r2 is the one.
  assert.deepEqual(placeOf(rows(12, -180, 1000)), { id: 'r2', top: 940 })
  // Scrolled into the rows' top margin: no row has reached the edge, so the first row is kept where it sits.
  assert.deepEqual(placeOf(rows(12, 500, 1000)), { id: 'r0', top: 1500 })
  assert.equal(placeOf([]), null)
})

test('a page of earlier rows moves the view down by exactly how far the kept row moved', () => {
  const place = placeOf(rows(20, -150, 150))!
  // 2400 px of earlier rows arrived above it: the kept row now starts 2400 px lower in the content.
  assert.equal(shiftTo(place, place.top + 2400), 2400)
  // Nothing above it moved — a picture loading below, the message box growing — so the view stays put.
  assert.equal(shiftTo(place, place.top), 0)
})

test('the reader scrolling their own view changes no row\'s start in the content', () => {
  // The same row, read at two scroll positions 200 px apart: the row is 200 px higher on the screen and the view is
  // 200 px further scrolled, so its start among the rows is the very same number.
  const at = { scrollTop: 700, getBoundingClientRect: () => ({ top: 200 }) } as unknown as HTMLElement
  const row = { getBoundingClientRect: () => ({ top: 230 }) } as unknown as HTMLElement
  const scrolled = { scrollTop: 900, getBoundingClientRect: () => ({ top: 200 }) } as unknown as HTMLElement
  const lowered = { getBoundingClientRect: () => ({ top: 30 }) } as unknown as HTMLElement
  assert.equal(contentTopOf(at, row), 730)
  assert.equal(contentTopOf(scrolled, lowered), 730)
  // A row 80 px into the content still reads as 80, whoever scrolls.
  assert.equal(contentTopOf(at, { getBoundingClientRect: () => ({ top: -420 }) } as unknown as HTMLElement), 80)
})

test('a place is paid once: the row\'s start is read back after the move, so a second pass moves nothing', () => {
  const place = placeOf(rows(20, -150, 150))!
  // 2400 px went in above the row. Moving the view by that much does NOT change where the row starts in the content
  // — that is the same number however the view is scrolled — so a place left as it was taken reads 2400 again on the
  // next render and moves the reader another 2400. That is the reported repeat (0, 2120, 4240, 6360, 8480, 10600).
  assert.equal(shiftTo(place, place.top + 2400), 2400)
  // Read back after the move, the place is where the row now is, and a second pass finds nothing left to move.
  const settled = settledPlace(place, place.top + 2400)
  assert.equal(shiftTo(settled, place.top + 2400), 0)
  // And a difference does not have to be closed by one move: a place read back after a short one is short too.
  assert.deepEqual(settledPlace(place, place.top), { id: place.id, top: place.top })
})

test('a view clamped short of the distance it was asked for gives up the rest, rather than owing it for ever', () => {
  // 5000 px went in above the row, but the view could only move 900 before the browser clamped it at the end: the
  // row never reached where it should, so where it actually got to is the place from now on.
  const place = { id: 'r7', top: 300 }
  const settled = settledPlace(place, 1200)
  assert.equal(shiftTo(settled, 1200), 0)
  // Not the rest of the debt: the 4100 px the view was short is dropped, not replayed on the next render.
  assert.equal(settled.top, 1200)
})

test('a follower keeps no place, because the rows are brought to the bottom after every render', () => {
  assert.equal(keptPlace(rows(20, -150), true), null)
  assert.equal(keptPlace(rows(20, -150), false)?.id, 'r2')
})

test('rows that cannot scroll are not asked for earlier ones', () => {
  // A short page: the view sits at its top permanently, so "near the top" is always true and says nothing.
  assert.equal(rowsCanScroll(400, 600), false)
  assert.equal(rowsCanScroll(600, 600), false)
  assert.equal(rowsCanScroll(900, 600), true)
})
