import assert from 'node:assert/strict'
import { test } from 'node:test'
import { hostHasPause, offersBotPause } from './host-pause.ts'

// The boundary is the point. A test that only tried the version the constant names would pass by construction,
// whatever the constant said: 2.54.0 is being released without the field, so that host must be offered no Pause
// unless it says `botPause`.
test('only a host above the release without the field can pause a bot by version alone', () => {
  assert.equal(hostHasPause('2.53.0'), false, 'the last released host has no paused field')
  assert.equal(hostHasPause('2.54.0'), false, 'release/2.54.0 carries no paused field either')
  assert.equal(hostHasPause('2.54.9'), false, 'a patch of that release is still that release')
  assert.equal(hostHasPause('2.55.0'), true, 'the first release that can honour it')
  assert.equal(hostHasPause('2.55.1'), true)
  assert.equal(hostHasPause('2.60.0'), true)
})

test('a flagless old host hides Pause, and a flagged host shows it', () => {
  assert.equal(offersBotPause('2.54.0', undefined), false, '2.54.0 omits the flag and would ignore paused')
  assert.equal(offersBotPause('2.54.0', false), false, 'an explicit false is not a host that honours it')
  assert.equal(offersBotPause('2.53.0'), false, 'the last release omits the flag')
  assert.equal(offersBotPause('2.54.9', undefined), false)
  assert.equal(offersBotPause(null, undefined), false, 'no hello yet')
  assert.equal(offersBotPause(null, false), false)
  assert.equal(offersBotPause('2.54.0', true), true, 'this build says so and stays on 2.54.0')
  assert.equal(offersBotPause(null, true), true, 'the flag is enough on its own')
  assert.equal(offersBotPause('2.55.0', undefined), true, 'a later release still shows it with the flag omitted')
  assert.equal(offersBotPause('2.55.0', false), true, 'the version path stays, so nothing that worked stops working')
  assert.equal(offersBotPause('2.60.0', true), true)
})
