import assert from 'node:assert/strict'
import test from 'node:test'
import { hostUpdateFixture } from './test-support/host-update.ts'
import { settle } from './test-support/main-module.ts'

for (const platform of ['darwin', 'win32']) {
  test(`${platform}: updating stops the bundled host after unregistering screen and persists restart`, async () => {
    const f = hostUpdateFixture({ platform })
    await f.host.prepareForUpdate()
    await settle()
    assert.deepEqual(f.order, ['unregisterScreen', 'stop'])
    assert.equal(f.persisted.hostRestartAfterAppUpdate, true)
    await f.host.refresh()
    assert.deepEqual(f.calls, [['stop']], 'refresh cannot restart the host during replacement')
    assert.equal(await f.host.isIdleForUpdate(), false)
  })
}

for (const options of [{ platform: 'linux' }, { devPort: '19333' }]) {
  test(`app replacement leaves externally managed hosts running: ${JSON.stringify(options)}`, async () => {
    const f = hostUpdateFixture(options)
    await f.host.prepareForUpdate()
    assert.equal(f.calls.length, 0)
    assert.equal(f.order.length, 0)
    assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), false)
  })
}

test('an install already in progress prevents update preparation from stopping the host', async () => {
  const f = hostUpdateFixture()
  const installing = f.host.install()
  await settle()
  await assert.rejects(f.host.prepareForUpdate(), /being installed/)
  assert.equal(f.calls.some((args) => args[0] === 'stop'), false)
  await f.tick(1000)
  await installing
})

test('missing bundled binary rejects preparation without pretending stop succeeded', async () => {
  const f = hostUpdateFixture({ missingBinary: true })
  await assert.rejects(f.host.prepareForUpdate(), /bundled host is missing/)
  assert.equal(f.calls.length, 0)
})

test('failed stop reports the error and cancellation reinstalls the service once', async () => {
  const f = hostUpdateFixture()
  f.behavior.stopStatus = 1
  await assert.rejects(f.host.prepareForUpdate(), /service command failed/)
  f.host.resumeAfterCancelledUpdate()
  f.host.resumeAfterCancelledUpdate()
  assert.deepEqual(f.calls, [['stop'], ['install', '--port', '19222']])
  await f.tick(1000)
})

test('new app launch reinstalls the host and clears restart marker only after health matches', async () => {
  const f = hostUpdateFixture({ restartMarker: true })
  const starting = f.host.refresh()
  await settle()
  assert.deepEqual(f.calls, [['install', '--port', '19222']])
  assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), true)
  f.behavior.reachable = false
  await f.tick(1000)
  await starting
  assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), true)
  f.behavior.reachable = true
  await f.tick(5000)
  assert.equal(f.host.state.kind, 'running')
  assert.equal(f.host.local?.computerId, 'fixture-computer')
  assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), false)
})

test('a stale host binary cannot satisfy the restart marker', async () => {
  const f = hostUpdateFixture({ restartMarker: true })
  f.behavior.wrongBinary = true
  const starting = f.host.refresh()
  await settle()
  await f.tick(1000)
  await starting
  assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), true)
  assert.notEqual(f.host.state.kind, 'running')
})

test('failed service installation keeps the restart marker and exposes the error', async () => {
  const f = hostUpdateFixture({ restartMarker: true })
  f.behavior.installStatus = 1
  await f.host.refresh()
  assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), true)
  assert.equal(f.host.state.kind, 'failed')
  if (f.host.state.kind === 'failed') assert.match(f.host.state.message, /service command failed/)
})

test('an uninstalled host does not acquire a restart marker', async () => {
  const f = hostUpdateFixture({ installed: false })
  await f.host.prepareForUpdate()
  assert.equal(f.prefs.get('hostRestartAfterAppUpdate'), false)
})

test('idle detection distinguishes working, offline and idle hosts', async () => {
  const f = hostUpdateFixture()
  assert.equal(await f.host.isIdleForUpdate(), true)
  f.behavior.busy = true
  assert.equal(await f.host.isIdleForUpdate(), false)
  f.behavior.reachable = false
  assert.equal(await f.host.isIdleForUpdate(), false)
})
