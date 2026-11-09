import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { ScreenState } from '../../shared/models'
import { accessPages, accessPageReady } from './access-onboarding.ts'

test('macOS setup combines three permissions and optional GitHub support', () => {
  assert.deepEqual(accessPages('darwin').map((page) => page.id), ['helper', 'capture', 'input', 'star'])
})

test('other desktops only show supported permissions followed by optional support', () => {
  assert.deepEqual(accessPages('linux').map((page) => page.id), ['helper', 'portal', 'star'])
  assert.deepEqual(accessPages('win32').map((page) => page.id), ['control', 'star'])
})

test('a completed permission can continue before the whole computer is ready', () => {
  const screen = { enabled: true, connected: true, capture: true, input: false } as ScreenState
  assert.equal(accessPageReady('helper', 'input', screen), true)
  assert.equal(accessPageReady('capture', 'input', screen), true)
  assert.equal(accessPageReady('input', 'input', screen), false)
  assert.equal(accessPageReady('portal', 'portal', screen), false)
  assert.equal(accessPageReady('helper', 'background', screen), false)
  assert.equal(accessPageReady('capture', 'host', screen), false)
  assert.equal(accessPageReady('capture', 'unavailable', screen), false)
  assert.equal(accessPageReady('helper', 'host', null), false)
  assert.equal(accessPageReady('star', 'ready', screen), false)
})
