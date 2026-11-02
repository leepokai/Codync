import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SNAP, fitCamera, snap, wheelZoomRatio, zoomAt } from './camera.ts'

test('fit frames nodes inside a fixed viewport', () => {
  const camera = fitCamera([{ x: 0, y: 0, width: 400, height: 200 }], { width: 800, height: 600 })
  assert.ok(Math.abs(camera.scale - 1.68) < 1e-9)
  assert.ok(camera.scale >= 0.25 && camera.scale <= 2)
})

test('an empty canvas fits the viewport center at 100%', () => {
  const camera = fitCamera([], { width: 800, height: 600 })
  assert.equal(camera.scale, 1)
  assert.equal(camera.x, 400)
  assert.equal(camera.y, 300)
})

test('zoom stays between 25% and 200%', () => {
  const shrunk = zoomAt({ x: 0, y: 0, scale: 0.25 }, { x: 10, y: 10 }, 0.1)
  const grown = zoomAt({ x: 0, y: 0, scale: 2 }, { x: 10, y: 10 }, 4)
  assert.equal(shrunk.scale, 0.25)
  assert.equal(grown.scale, 2)
})

test('pinch and a modified wheel use different ratios', () => {
  assert.equal(wheelZoomRatio({ deltaY: 8, deltaMode: 0, ctrlKey: true, metaKey: false }), 0.007)
  assert.equal(wheelZoomRatio({ deltaY: 120, deltaMode: 0, ctrlKey: false, metaKey: true }), 0.002)
  assert.equal(wheelZoomRatio({ deltaY: 40, deltaMode: 0, ctrlKey: false, metaKey: false }), null)
})

test('positions snap to 8', () => {
  assert.equal(snap(13), 16)
  assert.equal(snap(12), 16)
  assert.equal(SNAP, 8)
})
