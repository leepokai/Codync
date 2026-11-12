import assert from 'node:assert/strict'
import test from 'node:test'
import { clampZoom, FIT, MAX_SCALE, panBy, pinchFactor, pinchStep, toggleZoom, zoomBy } from './image-zoom.ts'

const box = { width: 1000, height: 800 }
const picture = { width: 600, height: 400 }
const center = { x: 0, y: 0 }

test('a picture smaller than the box stays centered at fit, and zooming in cannot push it out of the box', () => {
  assert.deepEqual(clampZoom({ scale: 1, x: 50, y: -20 }, box, picture), FIT)
  // 600 wide at 1.5x is 900: still inside 1000, so no pan.
  assert.deepEqual(clampZoom({ scale: 1.5, x: 40, y: 0 }, box, picture), { scale: 1.5, x: 0, y: 0 })
})

test('zoom is kept between fit and the maximum', () => {
  assert.equal(zoomBy(FIT, 0.2, center, box, picture).scale, 1)
  assert.equal(zoomBy(FIT, 100, center, box, picture).scale, MAX_SCALE)
})

test('zooming keeps the spot under the pointer where it was', () => {
  const big = { width: 2000, height: 1600 }
  const point = { x: 300, y: -100 }
  const start = { scale: 2, x: 100, y: 50 }
  const after = zoomBy(start, 1.5, point, box, big)
  // The picture's own coordinate under the pointer, before and after.
  const under = (z: typeof start) => ({ x: (point.x - z.x) / z.scale, y: (point.y - z.y) / z.scale })
  assert.ok(Math.abs(under(after).x - under(start).x) < 1e-9)
  assert.ok(Math.abs(under(after).y - under(start).y) < 1e-9)
})

test('panning stops at the picture edge', () => {
  const big = { width: 2000, height: 1600 }
  const zoomed = { scale: 2, x: 0, y: 0 }
  // 2000 * 2 = 4000 wide in a 1000 box: at most 1500 either way.
  assert.equal(panBy(zoomed, 5000, 0, box, big).x, 1500)
  assert.equal(panBy(zoomed, -5000, 0, box, big).x, -1500)
})

test('double-click zooms in on the spot, then back to fit', () => {
  const big = { width: 2000, height: 1600 }
  const zoomed = toggleZoom(FIT, { x: 100, y: 0 }, box, big)
  assert.equal(zoomed.scale, 2.5)
  assert.deepEqual(toggleZoom(zoomed, center, box, big), FIT)
})

test('a pinch out zooms in and a pinch in zooms out', () => {
  assert.ok(pinchFactor(-10) > 1)
  assert.ok(pinchFactor(10) < 1)
  assert.equal(pinchFactor(0), 1)
  // A wheel mouse's big steps are capped so one notch cannot jump to the maximum.
  assert.ok(pinchFactor(-500) < 2)
})

test('two fingers spreading zoom by their distance, and moving together pan', () => {
  const step = pinchStep([{ x: 0, y: 0 }, { x: 100, y: 0 }], [{ x: 10, y: 5 }, { x: 210, y: 5 }])
  assert.equal(step.factor, 2)
  assert.deepEqual(step.origin, { x: 50, y: 0 })
  assert.equal(step.dx, 60)
  assert.equal(step.dy, 5)
})
