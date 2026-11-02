import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ScreenViewport } from './screen-viewport.ts'

const near = (actual: number, expected: number) => assert.ok(Math.abs(actual - expected) < 1e-9, `${actual} ≠ ${expected}`)

test('a view wider than the display letterboxes the sides', () => {
  const viewport = new ScreenViewport({ width: 1000, height: 500 }, { width: 1000, height: 250 })
  near(viewport.scale, 0.5)
  assert.deepEqual(viewport.frame, { x: 250, y: 0, width: 500, height: 250 })
  assert.equal(viewport.toDisplay({ x: 100, y: 100 }), null, 'a click in the left bar is not sent')
  assert.equal(viewport.toDisplay({ x: 900, y: 100 }), null, 'a click in the right bar is not sent')
  assert.deepEqual(viewport.toDisplay({ x: 500, y: 125 }), { x: 500, y: 250 })
  assert.deepEqual(viewport.toDisplay({ x: 250, y: 0 }), { x: 0, y: 0 }, 'the video corner is on the display')
})

test('a view taller than the display letterboxes top and bottom', () => {
  const viewport = new ScreenViewport({ width: 1000, height: 500 }, { width: 500, height: 500 })
  near(viewport.scale, 0.5)
  assert.deepEqual(viewport.frame, { x: 0, y: 125, width: 500, height: 250 })
  assert.equal(viewport.toDisplay({ x: 250, y: 50 }), null, 'top bar')
  assert.equal(viewport.toDisplay({ x: 250, y: 450 }), null, 'bottom bar')
  assert.equal(viewport.toDisplay({ x: 250, y: 375 }), null, 'the frame ends at its bottom edge')
  assert.deepEqual(viewport.toDisplay({ x: 250, y: 250 }), { x: 500, y: 250 })
})

test('a point just inside the video maps, one just outside does not', () => {
  const viewport = new ScreenViewport({ width: 1920, height: 1080 }, { width: 960, height: 700 })
  const f = viewport.frame
  assert.notEqual(viewport.toDisplay({ x: 10, y: f.y + 0.5 }), null)
  assert.equal(viewport.toDisplay({ x: 10, y: f.y - 0.5 }), null)
})

test('a drag past the video edge clamps to the display', () => {
  const viewport = new ScreenViewport({ width: 1000, height: 500 }, { width: 1000, height: 250 })
  assert.deepEqual(viewport.clampToDisplay({ x: 0, y: -40 }), { x: 0, y: 0 })
  assert.deepEqual(viewport.clampToDisplay({ x: 1000, y: 999 }), { x: 999, y: 499 })
})

test('Retina: the video asks for twice the pixels, pointer points do not change', () => {
  const viewport = new ScreenViewport({ width: 1600, height: 900 }, { width: 800, height: 450 })
  assert.deepEqual(viewport.pixels(2), { width: 1600, height: 900 })
  assert.deepEqual(viewport.pixels(1), { width: 800, height: 450 })
  assert.deepEqual(viewport.pixels(1.5), { width: 1200, height: 675 })
  assert.deepEqual(viewport.pixels(0), { width: 800, height: 450 }, 'a bogus ratio counts as 1')
  assert.deepEqual(viewport.toDisplay({ x: 400, y: 225 }), { x: 800, y: 450 })
})

test('before layout the view is empty and nothing is NaN', () => {
  const viewport = new ScreenViewport({ width: 1000, height: 500 }, { width: 0, height: 0 })
  assert.equal(viewport.scale, 1)
  assert.equal(viewport.toDisplay({ x: 0, y: 0 }), null)
  assert.ok(Number.isFinite(viewport.pixels(2).width))
})
