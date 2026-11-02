import assert from 'node:assert/strict'
import { test } from 'node:test'
import { isPreparing, machineStatus, offlineCloudStatus, placementLine, type CloudMachine } from './cloud-computer.ts'

const machine: CloudMachine = {
  cloudComputerId: 'cm_1', provider: 'boat', providerName: 'Boat', region: 'EU', state: 'running',
  computerId: 'c1', backupAt: null, createdAt: 1,
}

test('a cloud computer names its provider and region', () => {
  assert.equal(placementLine(machine, (t) => t), 'Runs on Boat · EU')
})

test('an offline cloud computer that sleeps is not offline, and one that failed is an error', () => {
  assert.deepEqual(offlineCloudStatus({ ...machine, state: 'stopped' }), { text: 'Sleeping · wakes when you message', error: false })
  assert.deepEqual(offlineCloudStatus({ ...machine, state: 'stopping' }), { text: 'Sleeping · wakes when you message', error: false })
  assert.deepEqual(offlineCloudStatus({ ...machine, state: 'failed' }), { text: 'Cloud computer failed', error: true })
  assert.equal(offlineCloudStatus({ ...machine, state: 'starting' }), null)
  assert.equal(offlineCloudStatus(null), null)
  assert.equal(offlineCloudStatus(undefined), null)
})

test('a new cloud computer is preparing until it joins the account', () => {
  const fresh = { ...machine, computerId: null, state: 'starting' as const }
  assert.equal(machineStatus(fresh).text, 'Preparing your cloud computer…')
  assert.equal(isPreparing(fresh), true)
  assert.equal(isPreparing(machine), false)
  assert.equal(isPreparing(null), false)
  // A failed one isn't asked again, and says so.
  assert.equal(isPreparing({ ...fresh, state: 'failed' }), false)
  assert.deepEqual(machineStatus({ ...fresh, state: 'failed' }), { text: 'Failed', error: true })
})

test('a claimed cloud computer shows its state', () => {
  assert.equal(machineStatus(machine).text, 'Running')
  assert.equal(machineStatus({ ...machine, state: 'starting' }).text, 'Starting')
  assert.equal(machineStatus({ ...machine, state: 'stopping' }).text, 'Going to sleep')
  assert.equal(machineStatus({ ...machine, state: 'stopped' }).text, 'Sleeping')
  assert.deepEqual(machineStatus({ ...machine, state: 'failed' }), { text: 'Failed', error: true })
})
