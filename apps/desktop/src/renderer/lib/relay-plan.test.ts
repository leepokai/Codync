import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { CloudComputer } from '../store/cloud'
import { relayPlan } from './relay-plan.ts'

const computer = (id: string, access: string | null): CloudComputer => ({ computerId: id, name: id, signKey: '', access })
const stores = (entries: Record<string, boolean>) => (id: string) => (id in entries ? { isLoopback: entries[id]! } : null)

test('approved computers open through the relay, except this one and one already reached over SSH', () => {
  const plan = relayPlan([computer('here', 'granted'), computer('studio', 'granted'), computer('ssh', 'granted'), computer('asked', 'pending'), computer('new', 'none')], 'here', new Set(), stores({ here: true, ssh: true }))
  assert.deepEqual(plan.attach.map((c) => c.computerId), ['studio'])
  assert.deepEqual(plan.detach, [])
})

test('a relay store closes once its grant goes; one SSH took over is only forgotten', () => {
  const plan = relayPlan([computer('kept', 'granted'), computer('ssh', 'granted')], null, new Set(['kept', 'revoked', 'ssh']), stores({ kept: false, revoked: false, ssh: true }))
  assert.deepEqual(plan.attach, [])
  assert.deepEqual(plan.detach, ['revoked'])
  assert.deepEqual([...plan.kept], ['kept'])
})

test('when the SSH tunnel ends, the approved computer opens through the relay again', () => {
  const plan = relayPlan([computer('ssh', 'granted')], null, new Set(), stores({}))
  assert.deepEqual(plan.attach.map((c) => c.computerId), ['ssh'])
})
