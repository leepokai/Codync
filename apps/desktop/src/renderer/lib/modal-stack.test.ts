import assert from 'node:assert/strict'
import { test } from 'node:test'
import { stackModals, type PageLayer } from './modal-stack.ts'

const SHEET = 100
const DIALOG = 200
const layer = (el: string, overrides: Partial<PageLayer<string>> = {}): PageLayer<string> => ({ el, z: 0, modal: false, inert: false, muted: false, ...overrides })

test('a dialog stays on top of a sheet whichever opened first on the render', () => {
  for (const order of [['sheet', 'dialog'], ['dialog', 'sheet']]) {
    const layers = [layer('root'), ...order.map((el) => layer(el, { modal: true, z: el === 'dialog' ? DIALOG : SHEET }))]
    const { top, mute, unmute } = stackModals(layers)
    assert.equal(top, 'dialog')
    assert.deepEqual(mute.sort(), ['root', 'sheet'])
    assert.deepEqual(unmute, [])
  }
})

test('the later of two modals at the same z-index is on top', () => {
  const { top, mute } = stackModals([layer('root'), layer('a', { modal: true, z: SHEET }), layer('b', { modal: true, z: SHEET })])
  assert.equal(top, 'b')
  assert.deepEqual(mute, ['root', 'a'])
})

test('closing the top modal hands clicks to the next one down', () => {
  const { top, mute, unmute } = stackModals([
    layer('root', { inert: true, muted: true }),
    layer('sheet', { modal: true, z: SHEET, inert: true, muted: true }),
    layer('dialog', { z: DIALOG }),
  ])
  assert.equal(top, 'sheet')
  assert.deepEqual(unmute, ['sheet'])
  assert.deepEqual(mute, [])
})

test('a menu opened from the top modal comes back when a modal over it closes', () => {
  const { top, unmute } = stackModals([
    layer('root', { inert: true, muted: true }),
    layer('sheet', { modal: true, z: SHEET }),
    layer('menu', { z: 300, inert: true, muted: true }),
    layer('dialog', { z: DIALOG }),
  ])
  assert.equal(top, 'sheet')
  assert.deepEqual(unmute, ['menu'])
})

test('a menu open before the modal goes inert under it', () => {
  const { mute } = stackModals([layer('root'), layer('menu', { z: 300 }), layer('sheet', { modal: true, z: SHEET })])
  assert.deepEqual(mute, ['root', 'menu'])
})

test('the last close restores only the layers modals made inert', () => {
  const { top, mute, unmute } = stackModals([layer('root', { inert: true, muted: true }), layer('own', { inert: true }), layer('sheet')])
  assert.equal(top, null)
  assert.deepEqual(unmute, ['root'])
  assert.deepEqual(mute, [])
})
