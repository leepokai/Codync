import assert from 'node:assert/strict'
import { test } from 'node:test'
import { hostHasPause } from './host-pause.ts'

// The boundary is the point. A test that only tried the version the constant names would pass by construction,
// whatever the constant said: 2.54.0 is being released without the field, so that host must be offered no Pause.
test('only a host above the release without the field can pause a bot', () => {
  assert.equal(hostHasPause('2.53.0'), false, 'the last released host has no paused field')
  assert.equal(hostHasPause('2.54.0'), false, 'release/2.54.0 carries no paused field either')
  assert.equal(hostHasPause('2.54.9'), false, 'a patch of that release is still that release')
  assert.equal(hostHasPause('2.55.0'), true, 'the first release that can honour it')
  assert.equal(hostHasPause('2.55.1'), true)
  assert.equal(hostHasPause('2.60.0'), true)
})
