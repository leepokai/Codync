import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CACHE_LIMIT, cacheAttachment, cachedAttachment } from './attachment-cache.ts'

/** Bytes of `size`, without allocating them: the cache only reads their length. */
function bytes(size: number): Uint8Array {
  return { byteLength: size } as Uint8Array
}

test('the cache lets its oldest files go once it would pass its limit', () => {
  const first = bytes(CACHE_LIMIT)
  const second = bytes(1)
  cacheAttachment('first', first)
  cacheAttachment('second', second)
  assert.equal(cachedAttachment('first'), undefined, 'the oldest file went')
  assert.equal(cachedAttachment('second'), second)
})

test('a file bigger than the whole cache is not kept', () => {
  const big = bytes(CACHE_LIMIT + 1)
  cacheAttachment('big', big)
  assert.equal(cachedAttachment('big'), undefined)
})

test('a file read again is the last to go', () => {
  const first = bytes(CACHE_LIMIT / 2)
  const second = bytes(CACHE_LIMIT / 2)
  cacheAttachment('first', first)
  cacheAttachment('second', second)
  assert.equal(cachedAttachment('first'), first)
  const third = bytes(1)
  cacheAttachment('third', third)
  assert.equal(cachedAttachment('first'), first)
  assert.equal(cachedAttachment('second'), undefined, 'the one not read since went first')
})
