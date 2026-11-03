import assert from 'node:assert/strict'
import { test } from 'node:test'
import { closeViewerSession, ScreenInput, textChunks, type InputMessage, type PointerInfo } from './screen-input.ts'
import { ScreenViewport } from './screen-viewport.ts'

// A 1000×500 display in a 1000×250 view: the video is 500 wide at x 250…750, scale 0.5.
function setup() {
  const sent: InputMessage[] = []
  const input = new ScreenInput((m) => sent.push(m), new ScreenViewport({ width: 1000, height: 500 }, { width: 1000, height: 250 }))
  return { sent, input }
}
const pointer = (x: number, y: number, extra: Partial<PointerInfo> = {}): PointerInfo =>
  ({ x, y, button: 0, detail: 1, metaKey: false, altKey: false, ctrlKey: false, shiftKey: false, ...extra })
const keyInfo = { key: 'a', code: 'KeyA', metaKey: false, altKey: false, ctrlKey: false, shiftKey: false, altGraph: false, isComposing: false }

test('left, right and middle buttons go down and up at the display point', () => {
  const { sent, input } = setup()
  for (const [button, name] of [[0, 'left'], [2, 'right'], [1, 'middle']] as const) {
    assert.ok(input.pointerDown(pointer(500, 125, { button })))
    input.pointerUp(pointer(500, 125, { button }))
    assert.deepEqual(sent.splice(0), [
      { type: 'down', x: 500, y: 250, button: name, count: 1, modifiers: [] },
      { type: 'up', x: 500, y: 250, button: name, count: 1, modifiers: [] },
    ])
  }
})

test('a double click carries its count and the held modifiers', () => {
  const { sent, input } = setup()
  input.pointerDown(pointer(500, 125, { detail: 2, metaKey: true }))
  assert.deepEqual(sent[0], { type: 'down', x: 500, y: 250, button: 'left', count: 2, modifiers: ['cmd'] })
  input.pointerUp(pointer(500, 125, { detail: 2 }))
  input.pointerDown(pointer(500, 125, { detail: 9 }))
  assert.equal((sent[2] as { count: number }).count, 3, 'counts stop at three')
})

test('a click in a black bar is not sent', () => {
  const { sent, input } = setup()
  assert.equal(input.pointerDown(pointer(100, 125)), false)
  input.pointerUp(pointer(100, 125))
  input.pointerMove({ x: 100, y: 125 })
  input.wheel({ x: 100, y: 125, deltaX: 0, deltaY: 40, deltaMode: 0 })
  assert.deepEqual(sent, [])
})

test('moves follow the pointer over the video and skip repeats', () => {
  const { sent, input } = setup()
  input.pointerMove({ x: 500, y: 125 })
  input.pointerMove({ x: 500, y: 125 })
  input.pointerMove({ x: 502, y: 125 })
  assert.deepEqual(sent, [{ type: 'move', x: 500, y: 250 }, { type: 'move', x: 504, y: 250 }])
})

test('a drag keeps going past the edge, clamped, and ends where the pointer lets go', () => {
  const { sent, input } = setup()
  input.pointerDown(pointer(500, 125))
  input.pointerMove({ x: 500, y: 125 })
  input.pointerMove({ x: 1100, y: 125 })
  input.pointerUp(pointer(1100, 125))
  assert.deepEqual(sent, [
    { type: 'down', x: 500, y: 250, button: 'left', count: 1, modifiers: [] },
    { type: 'move', x: 999, y: 250 },
    { type: 'up', x: 999, y: 250, button: 'left', count: 1, modifiers: [] },
  ])
})

test('a second button while one is held is swallowed: the Mac helper tracks one', () => {
  const { sent, input } = setup()
  input.pointerDown(pointer(500, 125))
  assert.equal(input.pointerDown(pointer(500, 125, { button: 2 })), true)
  input.pointerUp(pointer(500, 125, { button: 2 }))
  assert.equal(sent.length, 1)
  assert.equal(input.pressed, true)
})

test('other mouse buttons are ignored', () => {
  const { sent, input } = setup()
  assert.equal(input.pointerDown(pointer(500, 125, { button: 3 })), false)
  assert.deepEqual(sent, [])
})

test('wheel and trackpad scroll are pixels at the pointer, horizontal too', () => {
  const { sent, input } = setup()
  input.wheel({ x: 500, y: 125, deltaX: 0, deltaY: 120, deltaMode: 0 })
  input.wheel({ x: 500, y: 125, deltaX: -30, deltaY: 0, deltaMode: 0 })
  input.wheel({ x: 500, y: 125, deltaX: 0, deltaY: 3, deltaMode: 1 })
  input.wheel({ x: 500, y: 125, deltaX: 0, deltaY: 1, deltaMode: 2 })
  input.wheel({ x: 500, y: 125, deltaX: 0, deltaY: 0, deltaMode: 0 })
  assert.deepEqual(sent, [
    { type: 'scroll', x: 500, y: 250, dx: 0, dy: 120, units: 'pixel' },
    { type: 'scroll', x: 500, y: 250, dx: -30, dy: 0, units: 'pixel' },
    { type: 'scroll', x: 500, y: 250, dx: 0, dy: 48, units: 'pixel' },
    { type: 'scroll', x: 500, y: 250, dx: 0, dy: 250, units: 'pixel' },
  ])
})

test('blur lifts a held button once, and does nothing when none is held', () => {
  const { sent, input } = setup()
  input.releaseAll()
  assert.deepEqual(sent, [])
  input.pointerDown(pointer(500, 125, { button: 2 }))
  input.pointerMove({ x: 600, y: 125 })
  input.releaseAll()
  input.releaseAll()
  assert.deepEqual(sent.slice(1), [
    { type: 'move', x: 700, y: 250 },
    { type: 'up', x: 700, y: 250, button: 'right', count: 1, modifiers: [] },
  ])
  assert.equal(input.pressed, false)
})

test('going view only lifts the held button and silences everything', () => {
  const { sent, input } = setup()
  input.pointerDown(pointer(500, 125))
  input.interactive = false
  assert.equal(sent.at(-1)?.type, 'up')
  const count = sent.length
  assert.equal(input.pointerDown(pointer(500, 125)), false)
  input.pointerMove({ x: 510, y: 125 })
  input.wheel({ x: 500, y: 125, deltaX: 0, deltaY: 9, deltaMode: 0 })
  input.text('hi')
  assert.equal(input.key(keyInfo, 'darwin', () => assert.fail('no paste')), false)
  assert.equal(sent.length, count)
})

test('keys: text, shortcuts and paste', () => {
  const { sent, input } = setup()
  let pasted = 0
  assert.equal(input.key(keyInfo, 'darwin', () => pasted++), true)
  assert.equal(input.key({ ...keyInfo, key: 'c', code: 'KeyC', metaKey: true }, 'darwin', () => pasted++), true)
  assert.equal(input.key({ ...keyInfo, key: 'v', code: 'KeyV', metaKey: true }, 'darwin', () => pasted++), true)
  assert.equal(input.key({ ...keyInfo, key: 'Shift', code: 'ShiftLeft', shiftKey: true }, 'darwin', () => pasted++), false)
  assert.deepEqual(sent, [{ type: 'text', text: 'a' }, { type: 'key', key: 'c', modifiers: ['cmd'] }])
  assert.equal(pasted, 1)
})

test('pasted text goes out in chunks that never split a character', () => {
  const emoji = '😀'
  const chunks = textChunks(emoji.repeat(5), 4)
  assert.deepEqual(chunks, [emoji + emoji, emoji + emoji, emoji])
  assert.equal(chunks.join(''), emoji.repeat(5))
  assert.deepEqual(textChunks(''), [])
  const { sent, input } = setup()
  input.text('x'.repeat(9000))
  assert.deepEqual(sent.map((m) => (m as { text: string }).text.length), [4000, 4000, 1000])
})

test('closing the viewer mid-drag sends the mouse-up through the session before it is let go', () => {
  const sent: InputMessage[] = []
  const log: string[] = []
  const viewer = { session: { send: (m: InputMessage) => { sent.push(m) }, close: () => { log.push('close') } } as { send(m: InputMessage): void; close(): void } | null }
  // The model's wiring: input reaches whatever session the viewer holds at that moment.
  const input = new ScreenInput((m) => viewer.session?.send(m), new ScreenViewport({ width: 1000, height: 500 }, { width: 1000, height: 250 }))
  input.pointerDown(pointer(500, 125))
  input.pointerMove(pointer(600, 125))
  sent.length = 0
  assert.ok(closeViewerSession(viewer, input))
  assert.deepEqual(sent.map((m) => m.type), ['up'])
  assert.equal(input.pressed, false)
  assert.equal(viewer.session, null)
  assert.deepEqual(log, ['close'])
})

test('closing a viewer that has no session sends nothing and does not throw', () => {
  const { sent, input } = setup()
  input.pointerDown(pointer(500, 125))
  sent.length = 0
  assert.equal(closeViewerSession({ session: null }, input), false)
  assert.deepEqual(sent, [], 'a held button is left for whoever ends the session')
})
