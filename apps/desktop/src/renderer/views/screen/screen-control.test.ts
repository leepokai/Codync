import assert from 'node:assert/strict'
import { test } from 'node:test'
import { ScreenControl, type ControlHost } from './screen-control.ts'

function setup(takeover: (on: boolean) => Promise<void> = async () => {}) {
  const calls: boolean[] = []
  const state = { session: true, live: true, input: true, refreshes: 0 }
  const host: ControlHost = {
    canControl: () => state.input && state.session,
    hasSession: () => state.session && state.live,
    takeover: (on) => { calls.push(on); return takeover(on) },
    update: () => {},
    refresh: () => { state.refreshes++ },
    fail: () => {},
  }
  const control = new ScreenControl(host)
  return { control, calls, state }
}
const settle = () => new Promise((resolve) => setTimeout(resolve, 0))

test('taking control asks the host once; handing it back asks again', async () => {
  const { control, calls } = setup()
  control.setMode('control')
  control.setMode('control')
  assert.equal(control.interactive, true)
  control.setMode('view')
  await settle()
  assert.deepEqual(calls, [true, false])
  assert.equal(control.interactive, false)
})

test('the host drops userControl after a reconnect: takeover is sent again, and the window only looks until it lands', async () => {
  let release: () => void = () => {}
  let hold = false
  const { control, calls } = setup(() => (hold ? new Promise<void>((resolve) => { release = resolve }) : Promise.resolve()))
  control.setMode('control')
  await settle()
  assert.equal(control.interactive, true)
  hold = true
  calls.length = 0
  control.sync(false)
  assert.deepEqual(calls, [true])
  assert.equal(control.interactive, false, 'not shown as controlling while the host has no takeover')
  release()
  await settle()
  assert.equal(control.interactive, true)
})

test('a status that still shows control, or a request in flight, sends nothing', async () => {
  let release: () => void = () => {}
  const { control, calls } = setup(() => new Promise<void>((resolve) => { release = resolve }))
  control.setMode('control')
  control.sync(false)
  assert.deepEqual(calls, [true], 'the takeover is still in flight')
  release()
  await settle()
  control.sync(true)
  assert.deepEqual(calls, [true])
  assert.equal(control.interactive, true)
})

test('a window that was not in control never asks for takeover after a reconnect', async () => {
  const { control, calls } = setup()
  control.setMode('view')
  control.sync(false)
  await settle()
  assert.deepEqual(calls, [])
  assert.equal(control.interactive, false)
})

test('no session: nothing is restored', async () => {
  const { control, calls, state } = setup()
  control.setMode('control')
  await settle()
  calls.length = 0
  state.session = false
  control.sync(false)
  assert.deepEqual(calls, [])
})

test('a restore the host refuses leaves the window looking, keeps the choice, and asks again only when the video is back', async () => {
  let fail = false
  const { control, calls } = setup(() => (fail ? Promise.reject(new Error('down')) : Promise.resolve()))
  control.setMode('control')
  await settle()
  fail = true
  control.sync(false)
  await settle()
  assert.deepEqual(calls, [true, true])
  assert.equal(control.interactive, false)
  assert.equal(control.preferred, 'control')
  control.sync(false)
  await settle()
  assert.equal(calls.length, 2, 'it does not keep asking on every status')
  fail = false
  control.resume()
  await settle()
  assert.equal(calls.length, 3)
  assert.equal(control.interactive, true)
})

test('a session that is reconnecting gets no takeover; once live it gets exactly one', async () => {
  const { control, calls, state } = setup()
  control.setMode('control')
  await settle()
  calls.length = 0
  state.live = false
  control.sync(false)
  control.sync(false)
  assert.deepEqual(calls, [])
  state.live = true
  control.sync(false)
  control.sync(false)
  assert.deepEqual(calls, [true])
})

test('a failed session hands control back to the bots: the window looks, and Retry asks again', async () => {
  const { control, calls } = setup()
  control.setMode('control')
  await settle()
  control.sessionFailed()
  assert.equal(control.interactive, false)
  assert.deepEqual(calls, [true, false], 'control is released even if a phone keeps the host session open')
  control.sessionFailed()
  assert.deepEqual(calls, [true, false], 'a second failure sends nothing more')
  control.set(control.preferred === 'control')
  await settle()
  assert.deepEqual(calls, [true, false, true])
  assert.equal(control.interactive, true)
})
