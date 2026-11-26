import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { BotDraft } from '../../../shared/models.ts'
import { visibleNetworkSwitches } from './network-switches.ts'

const draft = (rest: Partial<BotDraft> = {}) => ({ offeredToNetwork: false, reachableByAssistants: false, ...rest }) as BotDraft
test('the network switches are separate rows in settings order', () => {
  const rows = visibleNetworkSwitches(draft(), { joined: true, assistants: true })
  assert.deepEqual(rows.map((row) => row.title), ['Offered to the network', 'Reachable by cloud assistants'])
  assert.deepEqual(rows.map((row) => row.field), ['offeredToNetwork', 'reachableByAssistants'])
})

test('each switch needs both the draft field and the host network capability', () => {
  assert.deepEqual(visibleNetworkSwitches(draft(), { joined: true, assistants: true }).map((row) => row.title), [
    'Offered to the network',
    'Reachable by cloud assistants',
  ])
  assert.deepEqual(visibleNetworkSwitches(draft(), { joined: true }).map((row) => row.title), ['Offered to the network'])
  assert.deepEqual(visibleNetworkSwitches(draft(), { assistants: true }).map((row) => row.title), ['Reachable by cloud assistants'])
  assert.deepEqual(visibleNetworkSwitches(draft({ offeredToNetwork: undefined }), { joined: true, assistants: true }).map((row) => row.title), [
    'Reachable by cloud assistants',
  ])
})

test('no visible switches produce no rows, so CardSection adds no empty row', () => {
  assert.equal(visibleNetworkSwitches(draft(), null).length, 0)
  assert.equal(visibleNetworkSwitches(draft(), {}).length, 0)
})

test('an on switch is reported on and names the draft field it writes', () => {
  const [row] = visibleNetworkSwitches(draft({ offeredToNetwork: true }), { joined: true })
  assert.equal(row.on, true)
  assert.equal(row.field, 'offeredToNetwork')
})
