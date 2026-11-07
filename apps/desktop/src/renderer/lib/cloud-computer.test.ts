import assert from 'node:assert/strict'
import { test } from 'node:test'
import { cloudKeyOutcome, connectedProviders, isPreparing, machineStatus, offlineCloudStatus, placementLine, providersSummary, type CloudMachine, type CloudProvider } from './cloud-computer.ts'

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

test('a refused Boat key is told apart from a save that failed', () => {
  const refused = Object.assign(new Error('This Boat key has expired. Make a new one.'), { code: 'keyRefused' })
  assert.equal(cloudKeyOutcome(refused), 'refused')
  assert.equal(cloudKeyOutcome(Object.assign(new Error('Too many requests.'), { code: 'rateLimited' })), 'failed')
  assert.equal(cloudKeyOutcome(new TypeError('fetch failed')), 'failed')
})

test('the Cloud providers row names the connected providers, or asks for one', () => {
  const boat: CloudProvider = {
    provider: 'boat', name: 'Boat', region: 'EU', keyPrompt: 'boat_…',
    setup: { page: 'https://boat.dev/dashboard', commands: 'boat env new codync' }, credential: null,
  }
  const nimbus: CloudProvider = { ...boat, provider: 'nimbus', name: 'Nimbus' }
  assert.deepEqual(connectedProviders([boat, nimbus]), [])
  assert.deepEqual(providersSummary([boat, nimbus]), { text: 'Connect one', error: false })
  const keyed = { ...boat, credential: { hint: 'boat_…9abc', problem: null } }
  assert.deepEqual(connectedProviders([keyed, nimbus]), [keyed])
  assert.deepEqual(providersSummary([keyed, { ...nimbus, credential: { hint: 'nim_…0001', problem: null } }]), { text: 'Boat, Nimbus', error: false })
  assert.deepEqual(providersSummary([{ ...keyed, credential: { hint: 'boat_…9abc', problem: 'This Boat key has expired. Make a new one.' } }]), { text: 'Needs attention', error: true })
})
