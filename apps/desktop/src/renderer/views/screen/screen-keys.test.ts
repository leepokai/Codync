import assert from 'node:assert/strict'
import { test } from 'node:test'
import { keyCommand, type KeyInfo } from './screen-keys.ts'

function key(key: string, code: string, extra: Partial<KeyInfo> = {}): KeyInfo {
  return { key, code, metaKey: false, altKey: false, ctrlKey: false, shiftKey: false, altGraph: false, isComposing: false, ...extra }
}

test('a letter types as text', () => {
  assert.deepEqual(keyCommand(key('a', 'KeyA'), 'darwin'), { type: 'text', text: 'a' })
})

test('Shift+letter types the capital, in any layout', () => {
  assert.deepEqual(keyCommand(key('A', 'KeyA', { shiftKey: true }), 'darwin'), { type: 'text', text: 'A' })
  assert.deepEqual(keyCommand(key('Ф', 'KeyA', { shiftKey: true }), 'darwin'), { type: 'text', text: 'Ф' })
})

test('Cmd+C is a shortcut with the physical letter, even on a Cyrillic layout', () => {
  assert.deepEqual(keyCommand(key('c', 'KeyC', { metaKey: true }), 'darwin'), { type: 'key', key: 'c', modifiers: ['cmd'] })
  assert.deepEqual(keyCommand(key('с', 'KeyC', { metaKey: true }), 'darwin'), { type: 'key', key: 'c', modifiers: ['cmd'] })
  assert.deepEqual(keyCommand(key('z', 'KeyZ', { metaKey: true, shiftKey: true }), 'darwin'), { type: 'key', key: 'z', modifiers: ['cmd', 'shift'] })
  assert.deepEqual(keyCommand(key('c', 'KeyC', { ctrlKey: true }), 'linux'), { type: 'key', key: 'c', modifiers: ['ctrl'] })
})

test('a shortcut names the layout letter, not the key position (AZERTY Cmd+A sits on KeyQ)', () => {
  assert.deepEqual(keyCommand(key('a', 'KeyQ', { metaKey: true }), 'darwin'), { type: 'key', key: 'a', modifiers: ['cmd'] })
  assert.deepEqual(keyCommand(key('Z', 'KeyW', { metaKey: true, shiftKey: true }), 'darwin'), { type: 'key', key: 'z', modifiers: ['cmd', 'shift'] })
  // Option rewrites the character (Option+A is å): the physical letter is the fallback.
  assert.deepEqual(keyCommand(key('å', 'KeyA', { metaKey: true, altKey: true }), 'darwin'), { type: 'key', key: 'a', modifiers: ['cmd', 'option'] })
})

test('modifiers keep the phones order: cmd, option, ctrl, shift', () => {
  const all = key('a', 'KeyA', { metaKey: true, altKey: true, ctrlKey: true, shiftKey: true })
  assert.deepEqual(keyCommand(all, 'darwin'), { type: 'key', key: 'a', modifiers: ['cmd', 'option', 'ctrl', 'shift'] })
})

test('arrows, Enter, Backspace, Delete and function keys are named keys', () => {
  assert.deepEqual(keyCommand(key('ArrowLeft', 'ArrowLeft'), 'darwin'), { type: 'key', key: 'left', modifiers: [] })
  assert.deepEqual(keyCommand(key('ArrowUp', 'ArrowUp', { shiftKey: true }), 'darwin'), { type: 'key', key: 'up', modifiers: ['shift'] })
  assert.deepEqual(keyCommand(key('Enter', 'Enter'), 'darwin'), { type: 'key', key: 'return', modifiers: [] })
  assert.deepEqual(keyCommand(key('Enter', 'NumpadEnter'), 'win32'), { type: 'key', key: 'return', modifiers: [] })
  assert.deepEqual(keyCommand(key('Backspace', 'Backspace'), 'darwin'), { type: 'key', key: 'delete', modifiers: [] })
  assert.deepEqual(keyCommand(key('Delete', 'Delete'), 'darwin'), { type: 'key', key: 'forwardDelete', modifiers: [] })
  assert.deepEqual(keyCommand(key('F5', 'F5'), 'darwin'), { type: 'key', key: 'f5', modifiers: [] })
  assert.deepEqual(keyCommand(key('F12', 'F12', { ctrlKey: true }), 'linux'), { type: 'key', key: 'f12', modifiers: ['ctrl'] })
  assert.deepEqual(keyCommand(key('PageDown', 'PageDown'), 'darwin'), { type: 'key', key: 'pageDown', modifiers: [] })
  assert.deepEqual(keyCommand(key('Tab', 'Tab', { shiftKey: true }), 'darwin'), { type: 'key', key: 'tab', modifiers: ['shift'] })
  assert.deepEqual(keyCommand(key('Escape', 'Escape'), 'darwin'), { type: 'key', key: 'escape', modifiers: [] })
})

test('space types a space, and is a named key in a shortcut', () => {
  assert.deepEqual(keyCommand(key(' ', 'Space'), 'darwin'), { type: 'text', text: ' ' })
  assert.deepEqual(keyCommand(key(' ', 'Space', { metaKey: true }), 'darwin'), { type: 'key', key: 'space', modifiers: ['cmd'] })
})

test('Cmd+V pastes on the Mac, Ctrl+V elsewhere; the other chord is a plain shortcut', () => {
  assert.deepEqual(keyCommand(key('v', 'KeyV', { metaKey: true }), 'darwin'), { type: 'paste' })
  assert.deepEqual(keyCommand(key('v', 'KeyV', { ctrlKey: true }), 'win32'), { type: 'paste' })
  assert.deepEqual(keyCommand(key('v', 'KeyV', { ctrlKey: true }), 'darwin'), { type: 'key', key: 'v', modifiers: ['ctrl'] })
  assert.deepEqual(keyCommand(key('v', 'KeyV', { metaKey: true, shiftKey: true }), 'darwin'), { type: 'key', key: 'v', modifiers: ['cmd', 'shift'] })
})

test('a modifier alone, a dead key, an unknown key and a composing IME send nothing', () => {
  for (const name of ['Shift', 'Control', 'Alt', 'Meta', 'CapsLock', 'Dead', 'Process', 'Unidentified']) {
    assert.equal(keyCommand(key(name, name), 'darwin'), null, name)
  }
  assert.equal(keyCommand(key('MediaPlayPause', 'MediaPlayPause'), 'darwin'), null)
  assert.equal(keyCommand(key('n', 'KeyN', { isComposing: true }), 'darwin'), null)
})

test('AltGr characters type as text instead of Ctrl+Alt shortcuts', () => {
  assert.deepEqual(keyCommand(key('@', 'KeyQ', { ctrlKey: true, altKey: true, altGraph: true }), 'win32'), { type: 'text', text: '@' })
})
