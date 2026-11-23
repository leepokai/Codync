import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Backend } from '../../../shared/models.ts'
import { loginSeats } from './seats.ts'

const agent = (id: string, rest: Partial<Backend> = {}) => ({ id, name: id.toUpperCase(), available: true, ...rest }) as Backend
const claude = agent('claude', {
  defaultLoginProfile: { name: 'me@home' },
  loginProfiles: [{ id: 'w', name: 'Work Team', signedIn: true }, { id: 'x', name: 'Old', signedIn: false }],
} as Partial<Backend>)
const tr = (t: string) => t

test('each agent is its default login, then its signed-in login profiles', () => {
  const picked: unknown[] = []
  const seats = loginSeats({ backends: [claude, agent('codex')], on: { backend: 'claude', loginProfile: 'w' }, tr, pick: (b, id) => picked.push([b.id, id]) })
  assert.deepEqual(seats.map((s) => [s.key, s.name, s.badge, s.on]), [
    ['claude/', 'CLAUDE · me@home', undefined, false],
    ['claude/w', 'CLAUDE · Work Team', 'WT', true],
    ['codex/', 'CODEX', undefined, false],
  ])
  seats[2]?.pick()
  assert.deepEqual(picked, [['codex', null]])
})

test('the login in use stays even once signed out, and logins can be limited', () => {
  const on = { backend: 'claude', loginProfile: 'x' }
  assert.deepEqual(loginSeats({ backends: [claude], on, tr, pick: () => {} }).map((s) => s.key), ['claude/', 'claude/w', 'claude/x'])
  assert.deepEqual(loginSeats({ backends: [claude], on, tr, pick: () => {}, logins: () => ['w'] }).map((s) => s.key), ['claude/w'])
})
