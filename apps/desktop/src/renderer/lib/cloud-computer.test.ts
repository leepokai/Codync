import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  actionMachine, canCreate, canRemoveKey, chosenProvider, cloudCreateOutcome, cloudKeyOutcome, errorPlace, isPreparing, justClaimed, machineActions,
  limitCloudName, limitNote, machineStatus, offlineCloudStatus, placementLine, providerOption, suggestedName, type CloudMachine, type CloudProvider,
} from './cloud-computer.ts'

const machine: CloudMachine = {
  cloudComputerId: 'cm_1', name: 'Cloud computer', provider: 'boat', providerName: 'Boat', region: 'EU', state: 'running',
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

test("a cloud computer's menu renames it, opens its provider's key and deletes it; Delete snapshots only where its provider keeps some (older clouds do not say, and all kept some)", () => {
  const tr = (t: string) => t
  assert.deepEqual(machineActions(machine, tr).map((r) => r.action), ['rename', 'key', 'deleteSnapshots', 'delete'])
  assert.deepEqual(machineActions({ ...machine, snapshots: true }, tr).map((r) => r.action), ['rename', 'key', 'deleteSnapshots', 'delete'])
  assert.deepEqual(machineActions({ ...machine, provider: 'instabox', providerName: 'InstaBox', snapshots: false }, tr), [
    { action: 'rename', label: 'Rename…' },
    { action: 'key', label: 'InstaBox API key' },
    { action: 'delete', label: 'Delete cloud computer' },
  ])
  // In the app's language, the provider's name as the cloud gives it.
  const de = (t: string) => ({ '{provider} API key': '{provider}-API-Schlüssel' })[t] ?? t
  assert.equal(machineActions(machine, de)[1]?.label, 'Boat-API-Schlüssel')
})

test('a new cloud computer is preparing until it joins the account', () => {
  const fresh = { ...machine, computerId: null, state: 'starting' as const }
  assert.equal(machineStatus(fresh).text, 'Preparing your cloud computer…')
  assert.equal(isPreparing(fresh), true)
  assert.equal(isPreparing(machine), false)
  // A failed one isn't asked again, and says so.
  assert.equal(isPreparing({ ...fresh, state: 'failed' }), false)
  assert.deepEqual(machineStatus({ ...fresh, state: 'failed' }), { text: 'Failed', error: true })
  // It has no snapshots yet.
  const snapshots = (m: CloudMachine) => machineActions(m, (t) => t).find((a) => a.action === 'deleteSnapshots')?.disabled
  assert.equal(snapshots(fresh), true)
  assert.equal(snapshots(machine), false)
})

test('a claimed cloud computer shows its state', () => {
  assert.equal(machineStatus(machine).text, 'Running')
  assert.equal(machineStatus({ ...machine, state: 'starting' }).text, 'Starting')
  assert.equal(machineStatus({ ...machine, state: 'stopping' }).text, 'Going to sleep')
  assert.equal(machineStatus({ ...machine, state: 'stopped' }).text, 'Sleeping')
  assert.deepEqual(machineStatus({ ...machine, state: 'failed' }), { text: 'Failed', error: true })
})

test('a refused Boat key is told apart from a save that failed', () => {
  const refused = Object.assign(new Error('This key has expired. Make a new one.'), { code: 'keyRefused' })
  assert.equal(cloudKeyOutcome(refused), 'refused')
  assert.equal(cloudKeyOutcome(Object.assign(new Error('Too many requests.'), { code: 'rateLimited' })), 'failed')
  assert.equal(cloudKeyOutcome(new TypeError('fetch failed')), 'failed')
})

test("a cloud computer on a key its provider turns down says what to fix instead of its state", () => {
  assert.deepEqual(machineStatus(machine, 'This key has expired. Make a new one.'), { text: 'This key has expired. Make a new one.', error: true })
  assert.deepEqual(machineStatus(machine, null), machineStatus(machine))
})

test('a cloud computer that was preparing and now has its computer has just joined the account', () => {
  const fresh = { ...machine, computerId: null }
  assert.equal(justClaimed([fresh], [machine]), true)
  assert.equal(justClaimed([machine], [machine]), false)
  assert.equal(justClaimed([fresh], [fresh]), false)
  // A new one that arrives claimed wasn't seen preparing here: the create already listed it.
  assert.equal(justClaimed([], [machine]), false)
})

test('a new cloud computer is suggested the first free name', () => {
  const named = (name: string) => ({ ...machine, name })
  assert.equal(suggestedName([], (t) => t), 'Cloud computer')
  assert.equal(suggestedName([named('Work')], (t) => t), 'Cloud computer')
  assert.equal(suggestedName([named('Cloud computer')], (t) => t), 'Cloud computer 2')
  assert.equal(suggestedName([named('Cloud computer'), named('Cloud computer 3')], (t) => t), 'Cloud computer 2')
  assert.equal(suggestedName([named('Cloud computer 2'), named('Cloud computer')], (t) => t), 'Cloud computer 3')
  // In the app's language.
  const zh = (t: string) => ({ 'Cloud computer': '雲端電腦', 'Cloud computer {n}': '雲端電腦 {n}' })[t] ?? t
  assert.equal(suggestedName([named('雲端電腦')], zh), '雲端電腦 2')
})

test('a computer action is busy on its computer only; errors show where the action was', () => {
  assert.equal(actionMachine('delete:cm_1'), 'cm_1')
  assert.equal(actionMachine('deleteSnapshots:cm_2'), 'cm_2')
  assert.equal(actionMachine('rename:cm_3'), 'cm_3')
  assert.equal(actionMachine('create'), null)
  assert.equal(actionMachine('saveKey:boat'), null)
  assert.equal(actionMachine(null), null)
  assert.equal(errorPlace(null), 'card')
  assert.equal(errorPlace('delete:cm_1'), 'card')
  assert.equal(errorPlace('rename:cm_1'), 'card')
  assert.equal(errorPlace('create'), 'create')
  assert.equal(errorPlace('saveKey:boat'), 'key:boat')
  assert.equal(errorPlace('removeKey:instabox'), 'key:instabox')
})

test('a create the cloud answered with what to fix is refused, anything else failed', () => {
  const conflict = Object.assign(new Error('You can have up to 3 cloud computers. Delete one to make another.'), { status: 409, code: 'conflict' })
  assert.equal(cloudCreateOutcome(conflict), 'refused')
  assert.equal(cloudCreateOutcome(Object.assign(new Error('This key has expired. Make a new one.'), { status: 400, code: 'keyRefused' })), 'refused')
  assert.equal(cloudCreateOutcome(Object.assign(new Error("Couldn't reach the cloud provider."), { status: 503, code: 'providerUnavailable' })), 'failed')
  assert.equal(cloudCreateOutcome(new TypeError('fetch failed')), 'failed')
})

test('New cloud computer lists every provider and starts on the first with a usable key', () => {
  const boat: CloudProvider = {
    provider: 'boat', name: 'Boat', region: 'EU', keyPrompt: 'boat_…',
    setup: { page: 'https://boat.dev/dashboard', commands: 'boat env new codync' }, credential: null,
  }
  const instabox: CloudProvider = { ...boat, provider: 'instabox', name: 'InstaBox', region: 'US', credential: { hint: 'ibk_…0001', problem: null } }
  const refused = { ...boat, credential: { hint: 'boat_…9abc', problem: 'This key has expired. Make a new one.', blocking: true } }
  // The plan, not the key: what to fix shows, and Create tries again (a create that works clears it).
  const unpaid = { ...instabox, credential: { hint: 'ibk_…0001', problem: 'This account needs an active plan.', blocking: false } }
  const tr = (t: string) => t
  assert.deepEqual(providerOption(boat, tr), { usable: false, detail: { text: 'EU · Add a key first', error: false } })
  const offered = { ...boat, offer: { label: 'Get a free sandbox', detail: 'A free plan.', url: 'https://example.com' } }
  assert.deepEqual(providerOption(offered, tr), { usable: false, detail: { text: 'EU · Free sandbox available', error: false } })
  assert.deepEqual(providerOption(instabox, tr), { usable: true, detail: { text: 'US · ibk_…0001', error: false } })
  assert.deepEqual(providerOption(refused, tr), { usable: false, detail: { text: 'This key has expired. Make a new one.', error: true } })
  assert.deepEqual(providerOption(unpaid, tr), { usable: true, detail: { text: 'This account needs an active plan.', error: true } })
  assert.equal(chosenProvider([refused, unpaid], null), 'instabox')
  assert.equal(chosenProvider([boat, instabox], null), 'instabox')
  assert.equal(chosenProvider([refused, instabox], 'boat'), 'instabox')
  assert.equal(chosenProvider([boat, refused], null), null)
  const keyed = { ...boat, credential: { hint: 'boat_…9abc', problem: null } }
  assert.equal(chosenProvider([keyed, instabox], null), 'boat')
  assert.equal(chosenProvider([keyed, instabox], 'instabox'), 'instabox')
  // Picked while it had no key: chosen once it has one.
  assert.equal(chosenProvider([boat, instabox], 'boat'), 'instabox')
  assert.equal(chosenProvider([keyed, instabox], 'boat'), 'boat')
})

test('Create is on while the account may have another and nothing else runs; at the limit it says why', () => {
  assert.equal(canCreate(true, false, 2, 3), true)
  assert.equal(canCreate(true, false, 3, 3), false)
  assert.equal(canCreate(true, true, 0, 3), false)
  assert.equal(canCreate(false, false, 0, 3), false)
  assert.equal(limitNote(2, 3), null)
  // The cloud's own words for a create over the limit, which the catalogs translate.
  assert.equal(limitNote(3, 3), 'You can have up to 3 cloud computers. Delete one to make another.')
})

test('a name is cut to what the cloud keeps', () => {
  assert.equal(limitCloudName('Lab'), 'Lab')
  assert.equal(limitCloudName('x'.repeat(70)).length, 64)
})

test('a key stays while any cloud computer runs on its provider', () => {
  const onInstabox = { ...machine, cloudComputerId: 'cm_2', provider: 'instabox' }
  assert.equal(canRemoveKey([machine, onInstabox], 'boat'), false)
  assert.equal(canRemoveKey([machine, onInstabox], 'instabox'), false)
  assert.equal(canRemoveKey([onInstabox], 'boat'), true)
  assert.equal(canRemoveKey([], 'boat'), true)
})
