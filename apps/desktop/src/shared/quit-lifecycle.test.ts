import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import test from 'node:test'
import { QuitLifecycle } from '../main/quit-lifecycle.ts'

function fixture() {
  const app = new EventEmitter()
  const updater = new EventEmitter()
  const lifecycle = new QuitLifecycle(app, updater)
  const close = () => {
    let prevented = false
    let hidden = false
    lifecycle.closeToBackground({ preventDefault: () => { prevented = true } }, () => { hidden = true })
    return { prevented, hidden }
  }
  return { app, updater, lifecycle, close }
}

test('closing a window keeps the app running in the background', () => {
  const { close } = fixture()
  assert.deepEqual(close(), { prevented: true, hidden: true })
})

test('ordinary Quit lets the chat window close', () => {
  const { app, close } = fixture()
  app.emit('before-quit')
  assert.deepEqual(close(), { prevented: false, hidden: false })
})

test('a native update can close windows before app before-quit fires', () => {
  const { app, updater, close } = fixture()
  let appQuitStarted = false
  app.on('before-quit', () => { appQuitStarted = true })
  updater.emit('before-quit-for-update')
  assert.equal(appQuitStarted, false)
  assert.deepEqual(close(), { prevented: false, hidden: false })
  assert.deepEqual(close(), { prevented: false, hidden: false })
  app.emit('before-quit')
  assert.deepEqual(close(), { prevented: false, hidden: false })
})

test('checking and staging an update do not change ordinary close behavior', () => {
  const { updater, close } = fixture()
  for (const event of ['checking-for-update', 'update-available', 'update-downloaded', 'update-not-available']) {
    updater.emit(event)
    assert.deepEqual(close(), { prevented: true, hidden: true })
  }
})

test('explicit shutdown allows a previously hidden window to close', () => {
  const { lifecycle, close } = fixture()
  assert.deepEqual(close(), { prevented: true, hidden: true })
  lifecycle.beginQuit()
  assert.deepEqual(close(), { prevented: false, hidden: false })
})
