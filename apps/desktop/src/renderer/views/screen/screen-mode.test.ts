import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { ScreenState } from '../../../shared/models.ts'
import { ScreenInput, type InputMessage } from './screen-input.ts'
import { openingMode, thumbnailPhase } from './screen-mode.ts'
import { ScreenViewport } from './screen-viewport.ts'

const state = (extra: Partial<ScreenState>): ScreenState =>
  ({ enabled: true, connected: true, platform: 'macos', capture: true, input: true, displays: [], userControl: false, agentBot: null, computerUse: true, viewers: 0, ...extra })

test('a window opens in Take control, unless a bot is using the screen or control is not allowed', () => {
  assert.equal(openingMode(state({})), 'control')
  assert.equal(openingMode(state({ agentBot: 'bot-1' })), 'view', 'a bot is working on the screen')
  assert.equal(openingMode(state({ input: false })), 'view', 'no input permission')
  assert.equal(openingMode(state({ input: false, agentBot: 'bot-1' })), 'view')
  assert.equal(openingMode(null), 'view', 'no status yet')
})

test('the preview runs only while nothing else shows the screen and the app is visible', () => {
  assert.equal(thumbnailPhase({ windowOpen: false, hidden: false }), 'running')
  assert.equal(thumbnailPhase({ windowOpen: true, hidden: false }), 'window', 'the full viewer is open: the preview lets go')
  assert.equal(thumbnailPhase({ windowOpen: false, hidden: true }), 'idle', 'minimized')
  assert.equal(thumbnailPhase({ windowOpen: true, hidden: true }), 'idle')
})

test('a view-only input sends nothing: no pointer, wheel, key, paste or text', () => {
  const sent: InputMessage[] = []
  const input = new ScreenInput((m) => sent.push(m), new ScreenViewport({ width: 1000, height: 500 }, { width: 500, height: 250 }))
  input.interactive = false
  const point = { x: 250, y: 125, button: 0, detail: 1, metaKey: false, altKey: false, ctrlKey: false, shiftKey: false }
  assert.equal(input.pointerDown(point), false)
  input.pointerMove({ x: 260, y: 130 })
  input.pointerUp(point)
  input.wheel({ x: 250, y: 125, deltaX: 0, deltaY: 40, deltaMode: 0 })
  let pasted = false
  const taken = input.key({ key: 'v', code: 'KeyV', metaKey: true, altKey: false, ctrlKey: false, shiftKey: false, altGraph: false, isComposing: false }, 'darwin', () => { pasted = true })
  input.text('hello')
  assert.equal(taken, false, 'the browser keeps its own key handling')
  assert.equal(pasted, false)
  assert.deepEqual(sent, [])
})
