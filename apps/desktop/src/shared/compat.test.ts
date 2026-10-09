import assert from 'node:assert/strict'
import { test } from 'node:test'
import { checkVersions, isBelow, parseVersion } from './compat.ts'

test('versions parse like the host and the iPhone app', () => {
  assert.deepEqual(parseVersion('v2.6'), [2, 6, 0])
  assert.deepEqual(parseVersion('2.6.3-beta+1'), [2, 6, 3])
  assert.equal(parseVersion('2.x'), null)
  assert.equal(isBelow('2.6.3', '2.7.0'), true)
  assert.equal(isBelow('2.10.0', '2.9.9'), false)
  assert.equal(isBelow('garbage', '9.9.9'), false)
})

test('the side that must update', () => {
  assert.deepEqual(checkVersions('2.6.0', '2.7.0', '2.7.0'), { kind: 'updateApp', minimum: '2.7.0' })
  assert.deepEqual(checkVersions('2.7.0', '2.2.0', null), { kind: 'updateHost', version: '2.2.0', minimum: '2.12.0' })
  assert.equal(checkVersions('2.12.0', '2.12.0', '2.3.0'), null)
})
