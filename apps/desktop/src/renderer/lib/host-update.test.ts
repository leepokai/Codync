import assert from 'node:assert/strict'
import { test } from 'node:test'
import { HOST_UPDATE_WAIT, hostVersionLine, offerHostUpdate, type HostUpdateOffer } from './host-update.ts'

test('an update asked from here runs for a while, then is offered again', () => {
  const requested = { kind: 'requested', at: 1_000_000 } as const
  assert.deepEqual(offerHostUpdate(null, true, requested.at), { kind: 'available', last: null })
  assert.deepEqual(offerHostUpdate(requested, true, requested.at + HOST_UPDATE_WAIT - 1), { kind: 'updating' })
  assert.deepEqual(offerHostUpdate(requested, true, requested.at + HOST_UPDATE_WAIT), { kind: 'available', last: requested })
})

test('a refused update is offered again with why; a computer that cannot update from here is not', () => {
  const failed = { kind: 'failed', message: 'A bot is working on this computer. Update it once the bot is done.' } as const
  assert.deepEqual(offerHostUpdate(failed, true), { kind: 'available', last: failed })
  assert.deepEqual(offerHostUpdate(null, false), { kind: 'unavailable' })
})

test('the Codync row offers a newer release, says it is current, or makes no claim', () => {
  const line = (latest: string | null, offer: HostUpdateOffer) => hostVersionLine('2.28.0', latest, null, offer, 'Mac', 'macos')
  const available = { kind: 'available', last: null } as const
  assert.deepEqual(line('2.28.0', available), { version: '2.28.0', detail: ['Up to date'], warns: false, accessory: null })
  assert.deepEqual(line(null, available).detail, [])
  assert.deepEqual(line('2.29.0', available),
    { version: '2.28.0', detail: ['Version 2.29.0 is available.'], warns: false, accessory: { kind: 'button', title: 'Update' } })
  assert.deepEqual(line('2.29.0', { kind: 'unavailable' }).detail,
    ['Version 2.29.0 is available.', 'Open Codync on that Mac and choose Check for Updates.'])
  assert.deepEqual(line('2.29.0', { kind: 'updating' }).accessory, { kind: 'updating' })
})

test('a computer too old for this app, or an update that did not take, is a problem', () => {
  const behind = { kind: 'updateHost', version: '2.20.0', minimum: '2.26.0' } as const
  assert.deepEqual(hostVersionLine('2.20.0', null, behind, { kind: 'available', last: null }, 'Mac', 'linux'),
    { version: '2.20.0', detail: ['This app needs 2.26.0 or newer.'], warns: true, accessory: { kind: 'button', title: 'Update' } })
  const stalled = hostVersionLine('2.28.0', '2.29.0', null, { kind: 'available', last: { kind: 'requested', at: 0 } }, 'Mac', 'macos')
  assert.deepEqual(stalled, {
    version: '2.28.0', detail: ["Mac didn't finish updating. Try again, or update Codync on that computer."], warns: true,
    accessory: { kind: 'button', title: 'Try again' },
  })
})
