import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Point, PointerDragHandlers } from './pointer-drag.ts'

// A window and document just big enough for the drag: events, the root's classes, hit-testing.
const target = new EventTarget()
const classes = new Set<string>()
Object.assign(globalThis, {
  window: target,
  document: {
    documentElement: { classList: { add: (c: string) => classes.add(c), remove: (c: string) => classes.delete(c) } },
    elementFromPoint: () => null,
  },
})
const { pointerDrag } = await import('./pointer-drag.ts')

const fire = (type: string, props: Record<string, unknown> = {}) => target.dispatchEvent(Object.assign(new Event(type, { cancelable: true }), props))
/** Whether a click now reaches the page (`false`: the drag swallowed it). */
const clickGetsThrough = () => fire('click')
const nextTask = () => new Promise((resolve) => setTimeout(resolve, 0))
const press = (handlers: PointerDragHandlers) =>
  pointerDrag(handlers).onPointerDown({ button: 0, clientX: 0, clientY: 0, metaKey: false, ctrlKey: false, shiftKey: false } as never)

function recorder() {
  const calls: string[] = []
  return {
    calls,
    handlers: {
      start: (from: Point) => calls.push(`start ${from.x},${from.y}`),
      over: (at: Point) => calls.push(`over ${at.x},${at.y}`),
      drop: () => calls.push('drop'),
      cancel: () => calls.push('cancel'),
    },
  }
}

test('a press only becomes a drag past a few points, then reports moves and the drop', () => {
  const { calls, handlers } = recorder()
  press(handlers)
  fire('pointermove', { clientX: 2, clientY: 1 })
  assert.deepEqual(calls, [])
  fire('pointermove', { clientX: 0, clientY: 10 })
  assert.equal(classes.has('pointer-dragging'), true)
  fire('pointerup')
  assert.deepEqual(calls, ['start 0,0', 'over 0,10', 'drop'])
  assert.equal(classes.has('pointer-dragging'), false)
  // Nothing listens after the release.
  fire('pointermove', { clientX: 0, clientY: 40 })
  assert.equal(calls.length, 3)
})

test('a click without travel is not a drag', () => {
  const { calls, handlers } = recorder()
  press(handlers)
  fire('pointerup')
  assert.deepEqual(calls, [])
})

test('Escape or a cancelled pointer ends the drag without a drop', () => {
  const escape = recorder()
  press(escape.handlers)
  fire('pointermove', { clientX: 0, clientY: 10 })
  fire('keydown', { key: 'Escape' })
  fire('pointerup')
  assert.deepEqual(escape.calls, ['start 0,0', 'over 0,10', 'cancel'])

  const cancelled = recorder()
  press(cancelled.handlers)
  fire('pointermove', { clientX: 0, clientY: 10 })
  fire('pointercancel')
  assert.deepEqual(cancelled.calls, ['start 0,0', 'over 0,10', 'cancel'])
  assert.equal(classes.has('pointer-dragging'), false)
})

test('the click that ends a drag is swallowed, later clicks are not', async () => {
  const { handlers } = recorder()
  press(handlers)
  fire('pointermove', { clientX: 0, clientY: 10 })
  fire('pointerup')
  assert.equal(clickGetsThrough(), false)
  await nextTask()
  assert.equal(clickGetsThrough(), true)
})

test('after Escape, the release that ends the gesture does not click', async () => {
  const { handlers } = recorder()
  press(handlers)
  fire('pointermove', { clientX: 0, clientY: 10 })
  fire('keydown', { key: 'Escape' })
  fire('pointerup')
  assert.equal(clickGetsThrough(), false)
  await nextTask()
  assert.equal(clickGetsThrough(), true)
})

test('a plain click is never swallowed', () => {
  const { handlers } = recorder()
  press(handlers)
  fire('pointerup')
  assert.equal(clickGetsThrough(), true)
})
