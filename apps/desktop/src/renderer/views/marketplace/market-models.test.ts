import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appendPage, emptyPage, replacePage } from './market-models.ts'

test('catalog pages keep every fetched row and drop duplicates', () => {
  const first = replacePage([0, 1, 2], (n) => String(n))
  const next = appendPage(first, [2, 3, 3], (n) => String(n))
  assert.deepEqual(next.items, [0, 1, 2, 3])
  assert.deepEqual(replacePage([], (n: number) => String(n)).items, emptyPage().items)
})
