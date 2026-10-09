import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computerAccessStep, shouldSyncScreenAgent } from './computer-access.ts'
import type { ScreenState } from './models.ts'

const screen: ScreenState = { enabled: true, connected: true, capture: false, input: false, platform: 'macos', displays: [], userControl: false, agentBot: null, computerUse: true, viewers: 0 }
const agent = { available: true, needsApproval: false, registered: true }

test('startup never registers a fresh helper, but an app update restores an existing choice', () => {
  assert.equal(shouldSyncScreenAgent(true, 'not-registered', false), false)
  assert.equal(shouldSyncScreenAgent(true, 'not-registered', true), true)
  assert.equal(shouldSyncScreenAgent(true, 'enabled', false), true)
  assert.equal(shouldSyncScreenAgent(false, 'enabled', true), false)
  assert.equal(shouldSyncScreenAgent(true, 'requires-approval', true), false)
})

test('macOS setup advances only with observed permissions, and recovers after revocation', () => {
  assert.equal(computerAccessStep('darwin', false, screen, agent), 'host')
  assert.equal(computerAccessStep('darwin', true, null, agent), 'host')
  assert.equal(computerAccessStep('darwin', true, screen, { ...agent, available: false }), 'unavailable')
  assert.equal(computerAccessStep('darwin', true, { ...screen, enabled: false }, agent), 'enable')
  assert.equal(computerAccessStep('darwin', true, screen, { ...agent, registered: false }), 'enable')
  assert.equal(computerAccessStep('darwin', true, screen, { ...agent, registered: false, needsApproval: true }), 'background')
  assert.equal(computerAccessStep('darwin', true, { ...screen, connected: false }, agent), 'helper')
  assert.equal(computerAccessStep('darwin', true, screen, agent), 'capture')
  assert.equal(computerAccessStep('darwin', true, { ...screen, capture: true }, agent), 'input')
  assert.equal(computerAccessStep('darwin', true, { ...screen, capture: true, input: true }, agent), 'ready')
  assert.equal(computerAccessStep('darwin', true, { ...screen, input: true }, agent), 'capture')
})

test('Linux requests a portal session and Windows never asks for macOS grants', () => {
  assert.equal(computerAccessStep('linux', true, screen, null), 'portal')
  assert.equal(computerAccessStep('linux', true, { ...screen, capture: true }, null), 'portal')
  assert.equal(computerAccessStep('linux', true, { ...screen, capture: true, input: true }, null), 'ready')
  assert.equal(computerAccessStep('win32', true, { ...screen, connected: false, enabled: false }, null), 'ready')
})
