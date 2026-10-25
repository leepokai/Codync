import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appendPage, canPageForward, clampPage, emptyPage, needsFetch, pageCount, pageItems, replacePage } from './market-models.ts'

test('catalog pages keep every fetched row and drop duplicates', () => {
  const first = replacePage([0, 1, 2], (n) => String(n))
  const next = appendPage(first, [2, 3, 3], (n) => String(n))
  assert.deepEqual(next.items, [0, 1, 2, 3])
  assert.deepEqual(replacePage([], (n: number) => String(n)).items, emptyPage().items)
})

test('a catalog of 47 is five pages of 10, and the bar is never a single page while more rows exist', () => {
  const items = Array.from({ length: 47 }, (_, i) => i)
  assert.equal(pageCount(47), 5)
  assert.equal(pageCount(19), 2)
  assert.equal(pageCount(0), 1)
  assert.equal(pageCount(72, 10, true), 9)
  assert.deepEqual(pageItems(items, 0), items.slice(0, 10))
  assert.deepEqual(pageItems(items, 4), items.slice(40, 47))
  assert.deepEqual(pageItems(items, 9), [])
  assert.equal(clampPage(9, 47), 4)
  assert.equal(clampPage(9, 72, 10, true), 8)
  assert.equal(clampPage(-1, 20), 0)
  assert.equal(clampPage(4, 0), 0)
})

test('next stays available while the registry has another page', () => {
  assert.equal(canPageForward(10, 0, true), true)
  assert.equal(needsFetch(10, 1), true)
  assert.equal(canPageForward(20, 0, false), true)
  assert.equal(needsFetch(20, 1), false)
  assert.equal(canPageForward(8, 0, false), false)
  assert.equal(canPageForward(60, 5, true), true)
  assert.equal(needsFetch(60, 5), false)
  assert.equal(needsFetch(60, 6), true)
  assert.equal(needsFetch(72, 7), true)
  assert.equal(needsFetch(80, 7), false)
  assert.equal(needsFetch(0, 0), false)
  assert.equal(needsFetch(3, 0), false)
  assert.equal(needsFetch(0, 1), true)
})
