import assert from 'node:assert/strict'
import { test } from 'node:test'
import { edgeHits, edgeMidpoint, edgePath } from './edges.ts'

test('a line path contains both ends', () => {
  const path = edgePath({ x: 10, y: 20 }, { x: 110, y: 80 })
  assert.match(path, /M 10 20/)
  assert.match(path, /110 80/)
})

test('the midpoint sits between the ends and a hit is near the curve', () => {
  const from = { x: 0, y: 0 }
  const to = { x: 200, y: 0 }
  const mid = edgeMidpoint(from, to)
  assert.ok(mid.x > 40 && mid.x < 160)
  assert.ok(edgeHits(from, to, mid, 8))
  assert.equal(edgeHits(from, to, { x: 100, y: 80 }, 8), false)
})
