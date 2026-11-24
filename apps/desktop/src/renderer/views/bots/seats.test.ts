import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Backend } from '../../../shared/models.ts'
import { loginSeats, readySeats } from './seats.ts'

const agent = (id: string, rest: Partial<Backend> = {}) => ({ id, name: id.toUpperCase(), available: true, installed: true, ...rest }) as Backend
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

test('a chat or a new thread runs on installed agents and signed-in logins, and on default logins alone on an older host', () => {
  const out = { ...claude, id: 'gemini', name: 'GEMINI', signedIn: false, loginProfiles: undefined } as Backend
  const missing = agent('kiro', { available: false })
  const keys = (on: { backend: string; loginProfile?: string | null }, profiles?: boolean) =>
    readySeats({ backends: [{ ...claude, signedIn: false }, out, missing], on, tr, pick: () => {}, profiles }).map((s) => s.key)
  assert.deepEqual(keys({ backend: 'gemini' }), ['claude/w', 'gemini/'])
  assert.deepEqual(keys({ backend: 'claude', loginProfile: 'x' }), ['claude/w', 'claude/x'])
  assert.deepEqual(keys({ backend: 'kiro' }), ['claude/w', 'kiro/'])
  assert.deepEqual(keys({ backend: 'claude', loginProfile: 'w' }, false), ['claude/w', 'gemini/'])
})

test('an agent that is not installed says so', () => {
  assert.deepEqual(loginSeats({ backends: [agent('kiro', { available: false, installed: false })], on: { backend: 'claude' }, tr, pick: () => {} }).map((s) => s.name), ['KIRO (not installed)'])
})

test('an installed agent that is not ready is not called uninstalled', () => {
  const codex = agent('codex', { available: false, installed: true })
  assert.deepEqual(loginSeats({ backends: [codex], on: { backend: 'claude' }, tr, pick: () => {} }).map((s) => s.name), ['CODEX (not ready)'])
})

test('an installed agent missing Node names the dependency that blocks it', () => {
  const codex = agent('codex', { available: false, installed: true, node: 'missing' })
  assert.deepEqual(loginSeats({ backends: [codex], on: { backend: 'claude' }, tr, pick: () => {} }).map((s) => s.name), ['CODEX (needs Node.js)'])
})
