import assert from 'node:assert/strict'
import { test } from 'node:test'
import { centeringOffset, keptMatch, offsetToShow } from './find-position.ts'

const byPlace = (a: number, b: number) => a - b

test('the current match stays current when matches are added above it', () => {
  // On the third of [40, 50, 60]; loading earlier messages adds two before it.
  assert.equal(keptMatch([10, 20, 40, 50, 60], 60, byPlace), 4)
  assert.equal(keptMatch([10, 20, 40, 50, 60], 40, byPlace), 2)
})

test('a current match that went away hands over to the next one after it, or the last', () => {
  assert.equal(keptMatch([10, 30, 50], 20, byPlace), 1)
  assert.equal(keptMatch([10, 30, 50], 90, byPlace), 2)
  assert.equal(keptMatch([], 20, byPlace), 0)
})

test('a match is scrolled to the middle of the box', () => {
  assert.equal(centeringOffset({ start: 500, size: 20 }, { start: 100, size: 200 }), 310)
  assert.equal(centeringOffset({ start: 0, size: 20 }, { start: 100, size: 200 }), -190)
})

test('sideways, a match in sight stays put and one out of sight comes to the middle', () => {
  assert.equal(offsetToShow({ start: 150, size: 40 }, { start: 100, size: 400 }), 0)
  // Past the right edge of a long code line.
  assert.equal(offsetToShow({ start: 900, size: 40 }, { start: 100, size: 400 }), 620)
  // Partly cut off on the left.
  assert.equal(offsetToShow({ start: 80, size: 40 }, { start: 100, size: 400 }), -200)
})
