import assert from 'node:assert/strict'
import { test } from 'node:test'
import { DOT_RADIUS, dotGrid, dotRadius } from './dot-grid.ts'

test('below 2x dots sit on pixel centers (even pitch) or corners (odd) and the grid stays in its box', () => {
  for (const scale of [1, 1.25, 1.5]) {
    for (let px = 10; px <= 100; px++) {
      const size = px / scale
      const { cells, pitch, first } = dotGrid(size, scale)
      const devicePitch = Math.round(pitch * scale)
      assert.ok(Math.abs(pitch * scale - devicePitch) < 1e-6, `${px} px at ${scale}x: pitch ${pitch * scale}`)
      const offset = devicePitch % 2 ? 0 : 0.5
      for (let i = 0; i < cells; i++) {
        const center = (first + i * pitch) * scale
        assert.ok(Math.abs(center - Math.floor(center + 1e-6) - offset) < 1e-6, `${px} px at ${scale}x: dot ${i} at ${center}`)
      }
      const reach = DOT_RADIUS * pitch * scale
      assert.ok(first * scale - reach >= -0.5, `${px} px at ${scale}x overflows left`)
      assert.ok((first + (cells - 1) * pitch) * scale + reach <= px + 1, `${px} px at ${scale}x overflows right`)
    }
  }
})

test('below 2x no dot leaves its box, and an edge dot is cut by at most a pixel', () => {
  for (const scale of [1, 1.25, 1.5]) {
    for (let px = 10; px <= 200; px++) {
      const size = px / scale
      const { cells, pitch, first } = dotGrid(size, scale)
      for (let i = 0; i < cells; i++) {
        const x = first + i * pitch
        const r = dotRadius(pitch, 1, x, first, size)
        assert.ok(x - r >= -1e-9 && x + r <= size + 1e-9 && first - r >= -1e-9, `${px} px at ${scale}x: dot ${i} leaves the box`)
        assert.ok((pitch * DOT_RADIUS - r) * scale <= 1 + 1e-9, `${px} px at ${scale}x: dot ${i} cut by more than a pixel`)
      }
    }
  }
})

test('a 1x screen gets a sparser grid that still fills a menu bar icon', () => {
  assert.equal(dotGrid(30, 1).cells, 9)
  assert.equal(dotGrid(20, 1).cells, 7)
  assert.equal(dotGrid(20, 1).pitch, 3)
})

test('large avatars keep the 9 × 9 grid of large dots', () => {
  for (const scale of [1, 2, 3]) assert.equal(dotGrid(88, scale).cells, 9)
})

test('Retina keeps the exact design grid', () => {
  for (let size = 10; size <= 60; size += 0.5) {
    const cells = size < 18 ? 7 : 9
    const step = size / cells
    assert.deepEqual(dotGrid(size, 2), { cells, pitch: step, first: step / 2 })
  }
})
