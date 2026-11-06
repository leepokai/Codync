import assert from 'node:assert/strict'
import { register } from 'node:module'
import { test } from 'node:test'
import type { AppModel } from '../store/app-model'

await register(new URL('./resolve-hook.mjs', import.meta.url))

test('permission setup uses the local host identity even when a remote computer is selected', async () => {
  Object.assign(globalThis, { window: { codync: { computerName: 'OS computer name' } } })
  const local = { hostName: 'Local host' }
  const state = { local: local as typeof local | null, selectedStore: { hostName: 'Remote host' } }
  const { ComputerAccess } = await import('../store/computer-access.ts')
  const access = new ComputerAccess(state as unknown as AppModel)
  assert.equal(access.computerName, 'Local host')
  local.hostName = 'Renamed local host'
  assert.equal(access.computerName, 'Renamed local host')
  state.local = null
  assert.equal(access.computerName, 'OS computer name')
  window.codync.computerName = ''
  assert.equal(access.computerName, 'Your computer')
})

test('repeated checks never request permissions; explicit actions and failures stay reviewable', async () => {
  const calls: string[] = []
  let fail = false
  let capture = false
  let settings = ''
  Object.assign(globalThis, { window: { codync: { platform: 'darwin', app: {
    screenAgentState: async () => ({ available: true, needsApproval: false, registered: true }),
    openSettings: (url: string) => { settings = url },
  } } } })
  const app = { local: { connection: { kind: 'online' }, client: { call: async (method: string) => {
    calls.push(method)
    if (method === 'requestScreenPermission' && fail) throw new Error('Permission denied')
    return { enabled: true, connected: true, capture, input: false, permissionApp: 'CodyncDevScreen' }
  } } }, setRemoteScreen: async () => { calls.push('enable') }, screenError: null } as unknown as AppModel
  const { ComputerAccess } = await import('../store/computer-access.ts')
  const access = new ComputerAccess(app)
  await access.refresh()
  await access.refresh()
  assert.deepEqual(calls, ['screenStatus', 'screenStatus'])
  assert.equal(access.helperName, 'CodyncDevScreen')
  assert.equal(access.step, 'capture')
  access.openSettings('input')
  assert.match(settings, /Privacy_Accessibility/)
  assert.equal(access.step, 'capture')
  fail = true
  await access.request('capture')
  assert.equal(access.busy, null)
  assert.equal(access.error, 'Permission denied')
  assert.equal(access.requested.has('capture'), true)
  fail = false
  capture = true
  await access.request('capture')
  assert.equal(access.error, null)
  assert.equal(access.step, 'input')
  await access.enable()
  assert.ok(calls.includes('enable'))
})
