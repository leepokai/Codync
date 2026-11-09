import assert from 'node:assert/strict'
import { test } from 'node:test'
import { submenuHover, SUBMENU_HOVER_MS } from './submenu-hover.ts'

test('resting over a submenu row opens it without clicking', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const opened: (number | null)[] = []
  const hover = submenuHover((index) => opened.push(index))
  hover.schedule(1)
  t.mock.timers.tick(SUBMENU_HOVER_MS - 1)
  assert.deepEqual(opened, [])
  hover.schedule(1) // Moving inside the same row does not postpone opening.
  t.mock.timers.tick(1)
  assert.deepEqual(opened, [1])
})

test('crossing into the child cancels closing across the gap', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const opened: (number | null)[] = []
  const hover = submenuHover((index) => opened.push(index))
  hover.schedule(1)
  t.mock.timers.tick(SUBMENU_HOVER_MS)
  hover.schedule(null)
  t.mock.timers.tick(50)
  hover.cancel()
  t.mock.timers.tick(SUBMENU_HOVER_MS)
  assert.deepEqual(opened, [1])
  hover.schedule(null)
  t.mock.timers.tick(SUBMENU_HOVER_MS)
  assert.deepEqual(opened, [1, null])
})

test('passing another row or dismissing the menu cancels a pending open', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const opened: (number | null)[] = []
  const hover = submenuHover((index) => opened.push(index))
  hover.schedule(1)
  hover.schedule(2)
  t.mock.timers.tick(SUBMENU_HOVER_MS)
  assert.deepEqual(opened, [2])
  hover.schedule(3)
  hover.cancel()
  t.mock.timers.tick(SUBMENU_HOVER_MS)
  assert.deepEqual(opened, [2])
})
