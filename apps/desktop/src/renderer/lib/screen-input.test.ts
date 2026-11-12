import assert from 'node:assert/strict'
import { test } from 'node:test'
import { buttonName, displayPoint, fitted, isPaste, keyInput, modifiers, wheelPixels, type KeyPress } from './screen-input.ts'

const press = (key: string, code: string, keys: Partial<KeyPress> = {}): KeyPress =>
  ({ key, code, metaKey: false, altKey: false, ctrlKey: false, shiftKey: false, ...keys })

test('named keys go as key events with their modifiers', () => {
  assert.deepEqual(keyInput(press('Enter', 'Enter'), 'darwin', 'linux'), { type: 'key', key: 'return', modifiers: [] })
  assert.deepEqual(keyInput(press('Backspace', 'Backspace'), 'linux', 'linux'), { type: 'key', key: 'delete', modifiers: [] })
  assert.deepEqual(keyInput(press('ArrowLeft', 'ArrowLeft', { shiftKey: true }), 'darwin', 'macos'), { type: 'key', key: 'left', modifiers: ['shift'] })
  assert.deepEqual(keyInput(press('F5', 'F5'), 'win32', 'linux'), { type: 'key', key: 'f5', modifiers: [] })
})

test('plain characters and input methods are left to the text field', () => {
  assert.equal(keyInput(press('a', 'KeyA'), 'darwin', 'linux'), null)
  assert.equal(keyInput(press('A', 'KeyA', { shiftKey: true }), 'darwin', 'linux'), null)
  assert.equal(keyInput(press('Process', 'KeyA'), 'darwin', 'linux'), null)
  assert.equal(keyInput(press('a', 'KeyA', { isComposing: true, metaKey: true }), 'darwin', 'linux'), null)
})

test('shortcuts send the key under them, ⌘ as Ctrl off a Mac', () => {
  assert.deepEqual(keyInput(press('c', 'KeyC', { metaKey: true }), 'darwin', 'linux'), { type: 'key', key: 'c', modifiers: ['ctrl'] })
  assert.deepEqual(keyInput(press('c', 'KeyC', { metaKey: true }), 'darwin', 'macos'), { type: 'key', key: 'c', modifiers: ['cmd'] })
  assert.deepEqual(keyInput(press('Z', 'KeyZ', { metaKey: true, shiftKey: true }), 'darwin', 'macos'), { type: 'key', key: 'z', modifiers: ['cmd', 'shift'] })
  // With ⌘, Option still names the key under it.
  assert.deepEqual(keyInput(press('Ω', 'KeyZ', { metaKey: true, altKey: true }), 'darwin', 'linux'), { type: 'key', key: 'z', modifiers: ['ctrl', 'option'] })
  assert.deepEqual(keyInput(press('f', 'KeyF', { altKey: true }), 'linux', 'linux'), { type: 'key', key: 'f', modifiers: ['option'] })
  assert.deepEqual(keyInput(press(' ', 'Space', { ctrlKey: true }), 'linux', 'linux'), { type: 'key', key: 'space', modifiers: ['ctrl'] })
})

test('characters typed with Option or AltGr go as text', () => {
  assert.equal(keyInput(press('@', 'KeyL', { altKey: true }), 'darwin', 'linux'), null)
  assert.equal(keyInput(press('å', 'KeyA', { altKey: true, shiftKey: true }), 'darwin', 'linux'), null)
  const altGr = { ctrlKey: true, altKey: true, getModifierState: (key: string) => key === 'AltGraph' }
  assert.equal(keyInput(press('@', 'KeyQ', altGr), 'win32', 'linux'), null)
  assert.deepEqual(keyInput(press('ArrowLeft', 'ArrowLeft', { altKey: true }), 'darwin', 'linux'), { type: 'key', key: 'left', modifiers: ['option'] })
})

test('⌘ and Ctrl together name Ctrl once', () => {
  assert.deepEqual(modifiers({ metaKey: true, ctrlKey: true, altKey: false, shiftKey: false }, 'darwin', 'linux'), ['ctrl'])
})

test('paste is ⌘V on a Mac and Ctrl+V elsewhere', () => {
  assert.equal(isPaste(press('v', 'KeyV', { metaKey: true }), 'darwin'), true)
  assert.equal(isPaste(press('v', 'KeyV', { ctrlKey: true }), 'darwin'), false)
  assert.equal(isPaste(press('v', 'KeyV', { ctrlKey: true }), 'linux'), true)
})

test('a letterboxed picture maps to display points', () => {
  const box = { left: 0, top: 0, width: 1000, height: 1000 }
  // 16:10 video in a square box: bars above and below.
  assert.deepEqual(fitted(box, { width: 1600, height: 1000 }), { left: 0, top: 187.5, width: 1000, height: 625 })
  assert.deepEqual(displayPoint(500, 500, box, { width: 1600, height: 1000 }, { width: 1280, height: 800 }), { x: 640, y: 400 })
  assert.equal(displayPoint(500, 100, box, { width: 1600, height: 1000 }, { width: 1280, height: 800 }), null)
  assert.deepEqual(displayPoint(0, 187.5, box, { width: 1600, height: 1000 }, { width: 1280, height: 800 }), { x: 0, y: 0 })
})

test('buttons and wheel units', () => {
  assert.equal(buttonName(0), 'left')
  assert.equal(buttonName(1), 'middle')
  assert.equal(buttonName(2), 'right')
  assert.deepEqual(wheelPixels({ deltaX: 0, deltaY: 3, deltaMode: 1 }, 600), { dx: 0, dy: 48 })
  assert.deepEqual(wheelPixels({ deltaX: 2, deltaY: -5, deltaMode: 0 }, 600), { dx: 2, dy: -5 })
  assert.deepEqual(wheelPixels({ deltaX: 0, deltaY: 1, deltaMode: 2 }, 600), { dx: 0, dy: 600 })
})
