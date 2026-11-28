import assert from 'node:assert/strict'
import { test } from 'node:test'
import { Children, createElement, type ReactNode } from 'react'
import type { BotDraft } from '../../../shared/models.ts'
import { visibleNetworkSwitches, type NetworkSwitch } from './network-switches.ts'

/** What `CardSection` lays out (`Controls.tsx`): one `.card-row` per child, a `.hairline.inset` between them. */
function cardSectionRows(children: ReactNode) {
  const rows = Children.toArray(children)
  return { rows: rows.length, hairlines: Math.max(0, rows.length - 1) }
}

function switchChildren(rows: NetworkSwitch[]): ReactNode {
  return rows.map((row) => createElement('div', { key: row.key }, row.title))
}

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

test('CardSection gives 2, 1 and 0 switches their rows and hairlines', () => {
  const both = visibleNetworkSwitches(draft(), { joined: true, assistants: true })
  assert.deepEqual(cardSectionRows(switchChildren(both)), { rows: 2, hairlines: 1 })
  const one = visibleNetworkSwitches(draft(), { joined: true })
  assert.deepEqual(cardSectionRows(switchChildren(one)), { rows: 1, hairlines: 0 })
  const none = visibleNetworkSwitches(draft(), null)
  assert.deepEqual(cardSectionRows(switchChildren(none)), { rows: 0, hairlines: 0 })
})
