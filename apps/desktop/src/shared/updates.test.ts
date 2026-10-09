import assert from 'node:assert/strict'
import test from 'node:test'
import { EventEmitter } from 'node:events'
import { QuitLifecycle } from '../main/quit-lifecycle.ts'
import { deferred, settle } from './test-support/main-module.ts'
import { updaterFixture } from './test-support/updater.ts'

const release = { version: '2.11.3' }

for (const options of [{ packaged: false }, { main: false }]) {
  test(`development builds never check, download or install: ${JSON.stringify(options)}`, async () => {
    const f = updaterFixture(options)
    f.updates.check()
    await f.emit('update-downloaded', release)
    assert.equal(f.updates.state.supported, false)
    assert.equal(f.updates.state.canCheck, false)
    assert.equal(f.calls.check + f.calls.download + f.calls.install, 0)
    assert.equal(f.timers.size, 0)
  })
}

test('manual check broadcasts checking, release, staged states to every window', async () => {
  const f = updaterFixture()
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  f.ipc.emit('updates:check')
  assert.equal(f.updates.state.checking, true)
  assert.equal(f.handlers.get('updates:state')?.(), f.updates.state)
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1)
  assert.equal(f.updates.state.availableVersion, release.version)
  assert.equal(f.updates.state.checking, false)
  await f.emit('update-downloaded', release)
  assert.equal(f.updates.state.staged, true)
  assert.equal(f.calls.prepare, 1)
  assert.equal(f.calls.install, 0, 'must wait until the host stops')
  for (const window of [0, 1]) {
    const states = f.sent.filter((s) => s.window === window)
    assert.ok(states.every((s) => s.channel === 'updates:change'))
    assert.ok(states.some((s) => s.state.checking))
    assert.ok(states.some((s) => s.state.availableVersion === release.version))
    assert.ok(states.some((s) => s.state.staged))
  }
  stopped.resolve()
  await settle()
  assert.equal(f.calls.install, 1)
})

test('latest version records the last check without downloading or stopping the host', async () => {
  const f = updaterFixture()
  f.updates.check()
  await f.emit('update-not-available')
  assert.equal(f.updates.state.checking, false)
  assert.equal(f.updates.state.availableVersion, null)
  assert.equal(f.preferences.get('updatesLastCheck'), f.updates.state.lastCheck)
  assert.equal(f.calls.download + f.calls.prepare + f.calls.install, 0)
})

test('host stop failure preserves the staged release, reports an error and permits retry', async () => {
  const f = updaterFixture()
  f.behavior.prepare = async () => { throw new Error('stop failed') }
  f.updates.check()
  await f.emit('update-downloaded', release)
  assert.equal(f.calls.install, 0)
  assert.equal(f.calls.resume, 1)
  assert.equal(f.updates.state.error, 'Update paused: stop failed')
  assert.equal(f.updates.state.staged, true)
  f.behavior.prepare = async () => {}
  f.updates.check()
  await settle()
  assert.equal(f.calls.prepare, 2)
  assert.equal(f.calls.install, 1)
  assert.equal(f.updates.state.error, null)
  assert.equal(f.calls.download, 0, 'retry uses the staged archive')
})

test('installer error restarts the stopped host and retry prepares it again', async () => {
  const f = updaterFixture()
  f.updates.check()
  await f.emit('update-downloaded', release)
  await f.emit('error', new Error('signature rejected'))
  assert.equal(f.calls.resume, 1)
  assert.equal(f.updates.state.error, 'signature rejected')
  f.updates.check()
  await settle()
  assert.equal(f.calls.prepare, 2)
  assert.equal(f.calls.install, 2)
})

test('download errors are visible for manual checks without restarting a running host', async () => {
  const f = updaterFixture()
  f.updates.check()
  await f.emit('update-available', release)
  await f.emit('error', new Error('network disconnected'))
  assert.equal(f.updates.state.error, 'network disconnected')
  assert.equal(f.updates.state.checking, false)
  assert.equal(f.calls.prepare + f.calls.resume + f.calls.install, 0)
  f.updates.check()
  assert.equal(f.updates.state.error, null)
  assert.equal(f.calls.check, 2)
})

test('normal Quit without a staged update leaves the host alone', () => {
  const f = updaterFixture()
  let prevented = false
  f.app.emit('before-quit', { preventDefault: () => { prevented = true } })
  assert.equal(prevented, false)
  assert.equal(f.calls.prepare, 0)
})

test('Quit with a staged update waits for the host then retries Quit once', async () => {
  const f = updaterFixture()
  await f.emit('update-downloaded', release)
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  let prevented = false
  f.app.emit('before-quit', { preventDefault: () => { prevented = true } })
  assert.equal(prevented, true)
  assert.equal(f.calls.quit, 0)
  stopped.resolve()
  await settle()
  assert.equal(f.calls.quit, 1)
  assert.equal(f.calls.install, 0)
  f.app.emit('before-quit', { preventDefault: () => assert.fail('prepared Quit must proceed') })
})

test('failed preparation cancels Quit and reports a retryable error', async () => {
  const f = updaterFixture()
  await f.emit('update-downloaded', release)
  f.behavior.prepare = async () => { throw new Error('busy') }
  f.app.emit('before-quit', { preventDefault() {} })
  await settle()
  assert.equal(f.calls.quit, 0)
  assert.equal(f.calls.resume, 1)
  assert.match(f.updates.state.error!, /busy/)
})

test('automatic checks respect opt-out and run daily after the startup check', async () => {
  const f = updaterFixture()
  await f.tick(30_000)
  assert.equal(f.calls.check, 1)
  await f.emit('update-not-available')
  f.advance(3600_000)
  await f.tick(3600_000)
  assert.equal(f.calls.check, 1)
  f.advance(23 * 3600_000)
  await f.tick(3600_000)
  assert.equal(f.calls.check, 2)
  await f.emit('update-not-available')
  f.updates.setAutoCheck(false)
  f.advance(24 * 3600_000)
  await f.tick(3600_000)
  assert.equal(f.calls.check, 2)
  assert.equal(f.preferences.get('updatesAutoCheck'), false)
})

test('background discovery downloads only after automatic download is enabled', async () => {
  const f = updaterFixture()
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 0)
  f.ipc.emit('updates:autoDownload', {}, true)
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1)
  assert.equal(f.preferences.get('updatesAutoDownload'), true)
})

for (const block of ['disabled', 'focused', 'recentInput', 'busyHost'] as const) {
  test(`automatic install waits while ${block}, then installs once idle`, async () => {
    const f = updaterFixture({ prefs: { updatesAutoDownload: true } })
    if (block === 'disabled') f.updates.setAutoDownload(false)
    if (block === 'focused') f.behavior.focused = true
    if (block === 'recentInput') f.behavior.idleSeconds = 599
    if (block === 'busyHost') f.behavior.hostIdle = false
    await f.emit('update-downloaded', release)
    await f.tick(30_000)
    assert.equal(f.calls.prepare + f.calls.install, 0)
    f.updates.setAutoDownload(true)
    f.behavior.focused = false
    f.behavior.idleSeconds = 600
    f.behavior.hostIdle = true
    await f.tick(30_000)
    await f.tick(30_000)
    assert.equal(f.calls.prepare, 1)
    assert.equal(f.calls.install, 1)
  })
}

test('paired iPhones hold an incompatible release until the App Store catches up', async () => {
  const f = updaterFixture()
  f.updates.check()
  await f.emit('update-available', { ...release, minApp: '2.11.0' })
  assert.equal(f.updates.state.waitingForApp, '2.11.0')
  assert.match(f.updates.state.error!, /iPhone app 2.11.0/)
  assert.equal(f.calls.download, 0)
  f.behavior.storeVersion = '2.11.0'
  f.updates.check()
  await f.emit('update-available', { ...release, minApp: '2.11.0' })
  assert.equal(f.updates.state.waitingForApp, null)
  assert.equal(f.calls.download, 1)
})

for (const condition of ['noPhones', 'compatibleHost', 'manualOffline'] as const) {
  test(`compatibility permits download with ${condition}`, async () => {
    const f = updaterFixture()
    if (condition === 'noPhones') f.behavior.devices = []
    if (condition === 'compatibleHost') f.behavior.minApp = '2.11.0'
    if (condition === 'manualOffline') f.behavior.storeVersion = null
    f.updates.check()
    await f.emit('update-available', { ...release, minApp: '2.11.0' })
    assert.equal(f.updates.state.waitingForApp, null)
    assert.equal(f.calls.download, 1)
  })
}

test('unattended updates wait when App Store or paired-device information is unknown', async () => {
  const f = updaterFixture({ prefs: { updatesAutoDownload: true } })
  f.behavior.storeVersion = null
  f.behavior.hostFactsFail = true
  await f.emit('update-available', { ...release, minApp: '2.11.0' })
  assert.equal(f.calls.download, 0)
  assert.equal(f.updates.state.waitingForApp, '2.11.0')
  assert.equal(f.updates.state.error, null)
  f.advance(3600_000)
  await f.tick(3600_000)
  assert.equal(f.calls.check, 1, 'held releases recheck hourly')
})

test('repeated clicks during host shutdown must invoke the installer once', async () => {
  const f = updaterFixture()
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  f.updates.check()
  await f.emit('update-downloaded', release)
  f.updates.check()
  f.updates.check()
  stopped.resolve()
  await settle()
  assert.equal(f.calls.prepare, 1)
  assert.equal(f.calls.install, 1)
})

test('an updater error during host shutdown cancels installation and restores the host', async () => {
  const f = updaterFixture()
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  f.updates.check()
  await f.emit('update-downloaded', release)
  await f.emit('error', new Error('invalid archive'))
  stopped.resolve()
  await settle()
  assert.equal(f.calls.install, 0)
  assert.equal(f.calls.resume, 1)
  assert.equal(f.updates.state.error, 'invalid archive')
})

test('a synchronous installer failure restores the host and permits retry', async () => {
  const f = updaterFixture()
  f.behavior.install = () => { throw new Error('installer unavailable') }
  f.updates.check()
  await f.emit('update-downloaded', release)
  assert.equal(f.calls.resume, 1)
  assert.equal(f.updates.state.error, 'installer unavailable')
  f.behavior.install = () => {}
  f.updates.check()
  await settle()
  assert.equal(f.calls.prepare, 2)
  assert.equal(f.calls.install, 2)
})

test('an update error while preparing a normal Quit prevents that Quit', async () => {
  const f = updaterFixture()
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  await f.emit('update-downloaded', release)
  f.app.emit('before-quit', { preventDefault() {} })
  await f.emit('error', new Error('archive removed'))
  stopped.resolve()
  await settle()
  assert.equal(f.calls.quit, 0)
  assert.equal(f.calls.resume, 1)
  assert.equal(f.updates.state.error, 'archive removed')
})

test('manual installation crosses the native quit boundary after stopping the host', async () => {
  const f = updaterFixture()
  const nativeUpdater = new EventEmitter()
  const lifecycle = new QuitLifecycle(f.app, nativeUpdater)
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  let closed = false
  f.behavior.install = () => {
    nativeUpdater.emit('before-quit-for-update')
    lifecycle.closeToBackground(
      { preventDefault: () => assert.fail('native update must close the window') },
      () => assert.fail('native update must not hide the window'),
    )
    closed = true
    f.app.emit('before-quit', { preventDefault: () => assert.fail('host has already stopped') })
  }
  f.updates.check()
  await f.emit('update-downloaded', release)
  assert.equal(closed, false)
  stopped.resolve()
  await settle()
  assert.equal(closed, true)
  assert.equal(f.calls.install, 1)
})
