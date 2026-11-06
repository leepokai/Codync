import assert from 'node:assert/strict'
import test from 'node:test'
import { EventEmitter } from 'node:events'
import { QuitLifecycle } from '../main/quit-lifecycle.ts'
import { updateAction } from './update-action.ts'
import { deferred, settle } from './test-support/main-module.ts'
import { updaterFixture } from './test-support/updater.ts'

const release = { version: '2.11.3' }

for (const options of [{ packaged: false }, { main: false }]) {
  test(`development builds never check, download or install: ${JSON.stringify(options)}`, async () => {
    const f = updaterFixture(options)
    f.updates.check()
    await f.emit('update-downloaded', release)
    assert.equal(f.updates.state.supported, false)
    assert.equal(f.calls.check + f.calls.download + f.calls.install, 0)
    assert.equal(f.timers.size, 0)
  })
}

test('an idle macOS installer job whose update request is gone is removed, at launch and hourly', async () => {
  assert.equal(updaterFixture({ shipIt: { request: false, job: 'idle' } }).shipIt.removed, 1)
  for (const shipIt of [{ request: true, job: 'idle' }, { request: false, job: 'running' }, { request: false, job: 'none' }] as const) {
    assert.equal(updaterFixture({ shipIt }).shipIt.removed, 0, JSON.stringify(shipIt))
  }
  const f = updaterFixture()
  f.shipIt.request = false
  await f.tick(3600_000)
  assert.equal(f.shipIt.removed, 1)
})

test('manual check downloads with progress, then waits for Restart to update', async () => {
  const f = updaterFixture()
  const stopped = deferred()
  f.behavior.prepare = () => stopped.promise
  f.ipc.emit('updates:check')
  assert.equal(f.updates.state.checking, true)
  assert.equal(updateAction(f.updates.state).label, 'Checking…')
  assert.equal(f.handlers.get('updates:state')?.(), f.updates.state)
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1)
  assert.equal(f.updates.state.availableVersion, release.version)
  assert.equal(f.updates.state.checking, false)
  assert.equal(f.updates.state.downloadPercent, 0, 'the download shows at once')
  await f.emit('download-progress', { percent: 42.7 })
  assert.deepEqual(updateAction(f.updates.state), { label: 'Downloading… 42%', enabled: false })
  f.updates.check()
  assert.equal(f.calls.check, 1, 'a click while downloading does nothing')
  await f.emit('update-downloaded', release)
  assert.equal(f.updates.state.staged, true)
  assert.equal(f.updates.state.downloadPercent, null)
  assert.deepEqual(updateAction(f.updates.state), { label: 'Restart to update', enabled: true })
  assert.equal(f.calls.prepare, 0, 'a finished download never quits on its own')
  for (const window of [0, 1]) {
    const states = f.sent.filter((s) => s.window === window)
    assert.ok(states.every((s) => s.channel === 'updates:change'))
    assert.ok(states.some((s) => s.state.checking))
    assert.ok(states.some((s) => s.state.downloadPercent === 42))
    assert.ok(states.some((s) => s.state.staged))
  }
  f.updates.check()
  assert.equal(f.calls.prepare, 1)
  assert.equal(f.updates.state.installing, true)
  assert.equal(updateAction(f.updates.state).label, 'Restarting…')
  assert.equal(f.calls.install, 0, 'must wait until the host stops')
  stopped.resolve()
  await settle()
  assert.equal(f.calls.install, 1)
  assert.equal(f.preferences.get('updateRelaunchInFront'), true)
})

test('the Update badge downloads the release if needed, then restarts into it', async () => {
  const f = updaterFixture({ prefs: { updatesAutoDownload: false } })
  f.ipc.emit('updates:install')
  assert.equal(f.calls.check, 0, 'nothing to install before a release is found')
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 0)
  f.ipc.emit('updates:install')
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1)
  await f.emit('update-downloaded', release)
  assert.equal(f.calls.install, 1, 'restarts without a second click')
  assert.equal(f.preferences.get('updateRelaunchInFront'), true)
  assert.deepEqual({ ...f.preferences.get('updateInstalling') as object }, { version: release.version, at: f.now() })

  const staged = updaterFixture()
  await staged.emit('update-downloaded', release)
  staged.ipc.emit('updates:install')
  await settle()
  assert.equal(staged.calls.check, 0)
  assert.equal(staged.calls.install, 1)
})

test('the Update badge clicked during an automatic download restarts once it finishes', async () => {
  const f = updaterFixture()
  await f.emit('update-available', release)
  await f.emit('download-progress', { percent: 30 })
  f.ipc.emit('updates:install')
  assert.equal(f.calls.check, 0, 'the running download continues')
  await f.emit('update-downloaded', release)
  assert.equal(f.calls.install, 1)
})

test('another device asking this computer to update downloads the release and restarts into it once no bot is working', async () => {
  const f = updaterFixture({ prefs: { updatesAutoDownload: false } })
  let upToDate = 0
  f.updates.on('upToDate', () => upToDate++)
  f.host.emit('updateRequested')
  await f.emit('update-not-available')
  assert.equal(upToDate, 0, 'nobody at this computer asked, so nothing says up to date')
  f.host.emit('updateRequested')
  assert.equal(f.calls.check, 2)
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1, 'downloads even with automatic downloads off')
  f.behavior.focused = true
  f.behavior.hostIdle = false
  await f.emit('update-downloaded', release)
  await f.tick(30_000)
  assert.equal(f.calls.prepare + f.calls.install, 0, 'a working bot is never stopped')
  f.behavior.hostIdle = true
  await f.tick(30_000)
  assert.equal(f.calls.install, 1, "doesn't wait for the person at this computer")
  assert.notEqual(f.preferences.get('updateRelaunchInFront'), true, 'the app comes back in the background')

  const staged = updaterFixture()
  staged.behavior.focused = true
  await staged.emit('update-downloaded', release)
  staged.host.emit('updateRequested')
  await staged.tick(30_000)
  assert.equal(staged.calls.check, 0)
  assert.equal(staged.calls.install, 1)

  const lapsed = updaterFixture()
  lapsed.behavior.focused = true
  lapsed.behavior.hostIdle = false
  await lapsed.emit('update-downloaded', release)
  lapsed.host.emit('updateRequested')
  lapsed.advance(10 * 60_000)
  lapsed.behavior.hostIdle = true
  await lapsed.tick(30_000)
  assert.equal(lapsed.calls.install, 0, 'a request that lapsed waits for the person again')
})

test('an older copy opened while the installer runs quits and reopens the app once it stops', () => {
  const recent = { version: release.version, at: Date.UTC(2026, 9, 9) - 60_000 }
  for (const [name, marker, job] of [
    ['this version', { ...recent, version: '2.11.2' }, 'running'],
    ['marker too old', { ...recent, at: recent.at - 61_000 }, 'running'],
    ['installer idle', recent, 'idle'],
    ['installer gone', recent, 'none'],
  ] as const) {
    const f = updaterFixture({ prefs: { updateInstalling: marker }, shipIt: { request: true, job } })
    assert.equal(f.yieldsToInstall(), false, name)
    assert.equal(f.preferences.get('updateInstalling'), undefined, name)
    assert.deepEqual(f.spawned, [], name)
  }
  assert.equal(updaterFixture().yieldsToInstall(), false)
  const f = updaterFixture({ prefs: { updateInstalling: recent }, shipIt: { request: true, job: 'running' } })
  assert.equal(f.yieldsToInstall(), true)
  assert.deepEqual(f.preferences.get('updateInstalling'), recent)
  assert.deepEqual(f.spawned.map((args) => args.slice(2)), [['com.pokai.Codync.ShipIt', '/Applications/Codync.app']])
})

test('the app relaunched by a chosen restart opens its own bundle once to come forward', () => {
  const relaunched = updaterFixture({ prefs: { updateRelaunchInFront: true } })
  assert.deepEqual(relaunched.opened, ['/Applications/Codync.app'])
  assert.equal(relaunched.preferences.get('updateRelaunchInFront'), false)
  assert.deepEqual(updaterFixture().opened, [])
})

test('latest version records the last check without downloading or stopping the host', async () => {
  const f = updaterFixture()
  let upToDate = 0
  f.updates.on('upToDate', () => upToDate++)
  f.updates.check()
  await f.emit('update-not-available')
  assert.equal(upToDate, 1, 'the check the person asked for says so')
  assert.equal(f.updates.state.checking, false)
  assert.equal(f.updates.state.availableVersion, null)
  assert.equal(f.preferences.get('updatesLastCheck'), f.updates.state.lastCheck)
  assert.equal(f.calls.download + f.calls.prepare + f.calls.install, 0)
})

test('a latest release whose build is still uploading counts as up to date, not an error', async () => {
  const f = updaterFixture()
  let upToDate = 0
  f.updates.on('upToDate', () => upToDate++)
  f.updates.check()
  await f.emit('error', Object.assign(new Error('Cannot find latest-mac.yml in the latest release artifacts'), { code: 'ERR_UPDATER_CHANNEL_FILE_NOT_FOUND' }))
  assert.equal(upToDate, 1)
  assert.equal(f.updates.state.error, null)
  assert.equal(f.updates.state.checking, false)
  assert.equal(f.updates.state.availableVersion, null)
  assert.equal(f.calls.download, 0)
})

test('host stop failure preserves the staged release, reports an error and permits retry', async () => {
  const f = updaterFixture()
  f.behavior.prepare = async () => { throw new Error('stop failed') }
  await f.emit('update-downloaded', release)
  f.updates.check()
  await settle()
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
  await f.emit('update-downloaded', release)
  f.updates.check()
  await settle()
  assert.deepEqual({ ...f.preferences.get('updateInstalling') as object }, { version: release.version, at: f.now() })
  await f.emit('error', new Error('signature rejected'))
  assert.equal(f.calls.resume, 1)
  assert.equal(f.updates.state.error, 'signature rejected')
  assert.equal(f.updates.state.installing, false)
  assert.equal(f.preferences.get('updateRelaunchInFront'), false)
  assert.equal(f.preferences.get('updateInstalling'), undefined)
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
  assert.deepEqual({ ...f.preferences.get('updateInstalling') as object }, { version: release.version, at: f.now() }, 'an old copy opened while it installs yields')
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
  let upToDate = 0
  f.updates.on('upToDate', () => upToDate++)
  await f.tick(30_000)
  assert.equal(f.calls.check, 1)
  await f.emit('update-not-available')
  assert.equal(upToDate, 0, 'scheduled checks stay silent')
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

test('background discovery downloads by default and not once automatic download is off', async () => {
  const f = updaterFixture()
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1)
  f.ipc.emit('updates:autoDownload', {}, false)
  await f.emit('update-available', release)
  assert.equal(f.calls.download, 1)
  assert.equal(f.preferences.get('updatesAutoDownload'), false)
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
  await f.emit('update-downloaded', release)
  f.updates.check()
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
  await f.emit('update-downloaded', release)
  f.updates.check()
  await settle()
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
  await f.emit('update-downloaded', release)
  f.updates.check()
  await settle()
  assert.equal(closed, false)
  stopped.resolve()
  await settle()
  assert.equal(closed, true)
  assert.equal(f.calls.install, 1)
})
