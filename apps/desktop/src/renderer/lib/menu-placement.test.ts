import assert from 'node:assert/strict'
import { test } from 'node:test'
import { menuPlacement, submenuPlacement } from './menu-placement.ts'

const size = { width: 260, height: 120 }
const parent = { left: 30, top: 80, right: 290, width: 260, height: 240 }

test('hovered submenu opens beside the parent aligned with its row', () => {
  assert.deepEqual(submenuPlacement(parent, { top: 120 }, size, { width: 1000, height: 700 }),
    { left: 294, top: 113, width: 260 })
})

test('submenu flips left at the right edge and stays above the bottom edge', () => {
  assert.deepEqual(submenuPlacement({ ...parent, left: 700, right: 960 }, { top: 650 }, size, { width: 1000, height: 700 }),
    { left: 436, top: 572, width: 260 })
})

test('small windows keep both menus inside the viewport', () => {
  const view = { width: 240, height: 200 }
  const placed = menuPlacement({ x: 220, y: 190 }, size, view)
  assert.deepEqual(placed, { left: 8, top: 72, width: 224 })
  const sub = submenuPlacement({ ...parent, ...placed, right: placed.left + placed.width }, { top: 100 }, size, view)
  assert.equal(sub.left, 8)
  assert.equal(sub.width, 224)
})
