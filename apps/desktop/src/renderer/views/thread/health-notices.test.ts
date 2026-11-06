import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from '../../../shared/models.ts'
import { newestHealthNotices } from './health-notices.ts'

const notice = (id: string, text: string, data: Record<string, unknown> = {}): Entry => ({
  id,
  botId: 'b',
  rev: 1,
  kind: 'notice',
  turn: 0,
  createdAt: 0,
  updatedAt: 0,
  seq: 0,
  data: { text, ...data },
})

test('only the newest memory banner survives, and other notices stay', () => {
  const entries = [
    notice('a', "This computer's memory is 84 % full (warning from 70 %)."),
    notice('user', 'Hello'),
    notice('b', "This computer's memory is 86 % full, above the 85 % limit."),
    notice('c', "This computer's memory is 81 % full and easing below the warning level."),
    notice('d', 'Created routine: Saludo'),
  ]
  const kept = newestHealthNotices(entries).map((e) => e.id)
  assert.deepEqual(kept, ['user', 'c', 'd'])
})

test('a health notice marked by the host is collapsed too, and no banners is a no-op', () => {
  const entries = [
    notice('a', 'Memory is full.', { health: true }),
    notice('b', 'Memory is still full.', { health: true }),
  ]
  assert.deepEqual(newestHealthNotices(entries).map((e) => e.id), ['b'])
  assert.deepEqual(newestHealthNotices([notice('x', 'Hello')]).map((e) => e.id), ['x'])
})
