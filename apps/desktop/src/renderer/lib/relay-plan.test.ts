import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { CloudComputer } from '../store/cloud'
import { relayComputers } from './relay-plan.ts'

const computer = (id: string, access: string | null): CloudComputer => ({ computerId: id, name: id, signKey: '', access })

test('approved computers open through the relay, except this one and one already reached over SSH', () => {
  const computers = [computer('here', 'granted'), computer('studio', 'granted'), computer('ssh', 'granted'), computer('asked', 'pending'), computer('new', 'none')]
  const ids = relayComputers(computers, 'here', (id) => id !== 'ssh').map((c) => c.computerId)
  assert.deepEqual(ids, ['studio'])
})

test('when the SSH tunnel ends, the approved computer opens through the relay again', () => {
  assert.deepEqual(relayComputers([computer('ssh', 'granted')], null, () => true).map((c) => c.computerId), ['ssh'])
})
