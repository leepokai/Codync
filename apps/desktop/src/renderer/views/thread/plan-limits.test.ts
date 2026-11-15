import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Usage } from '../../../shared/models.ts'
import { planLimits, untilShort } from './plan-limits.ts'

test('time left reads 2h55, 45m or 3d', () => {
  const now = 1_000_000_000_000
  assert.equal(untilShort(now + (2 * 3600 + 55 * 60) * 1000, now), '2h55')
  assert.equal(untilShort(now + 45 * 60 * 1000, now), '45m')
  assert.equal(untilShort(now + (3 * 24 + 2) * 3600 * 1000, now), '3d')
  assert.equal(untilShort(now - 5000, now), '0m')
})

const now = Date.UTC(2026, 10, 15, 12, 0, 0)
const usage: Usage = {
  providers: [
    {
      id: 'claude',
      name: 'Claude',
      source: 'test',
      updatedAt: 0,
      windows: [
        { id: 'five', label: '5-hour', percent: 59, resetsAt: now + (2 * 60 + 55) * 60_000 },
        { id: 'week', label: 'Weekly', percent: 26, resetsAt: now + 6 * 86_400_000 + 3_600_000 },
        { id: 'opus', label: 'Weekly · Opus', percent: 10, resetsAt: now + 6 * 86_400_000 },
        { id: 'tokens', label: 'Today', percent: 0, text: '12k tokens' },
      ],
      loginProfiles: [{ id: 'perso', name: 'Perso', updatedAt: 0, windows: [{ id: 'five', label: '5-hour', percent: 95, resetsAt: now + 600_000 }] }],
    },
  ],
}

test("a bot shows its login's limits: the default login's, or its profile's", () => {
  const rows = planLimits(usage, 'claude', null, now)
  assert.deepEqual(rows.map((r) => [r.title, r.percent, r.reset]), [['Session', 59, '2h55'], ['Week', 26, '6d'], ['Opus', 10, '6d']], 'notes are not limits')
  const perso = planLimits(usage, 'claude', 'perso', now)
  assert.deepEqual(perso.map((r) => [r.title, r.percent, r.level, r.reset]), [['Session', 95, 'over', '10m']])
  assert.deepEqual(planLimits(usage, 'claude', 'unknown', now), [], 'a profile with no reading shows nothing')
  assert.deepEqual(planLimits(usage, 'codex', null, now), [], 'another agent')
})

test('the week carries its day and the share a steady pace allows', () => {
  const [, week] = planLimits(usage, 'claude', null, now)
  assert.equal(week.day, 1)
  assert.equal(week.pace, 14.3)
  assert.equal(week.level, 'over', '26% on the first day is ahead of 14.3%')
  const later = planLimits(usage, 'claude', null, now + 3 * 86_400_000)[1]
  assert.equal(later.day, 4)
  assert.equal(later.pace, 57.1)
  assert.equal(later.level, 'ok')
  const near = { ...usage, providers: [{ ...usage.providers[0], windows: [{ id: 'w', label: 'Weekly', percent: 45, resetsAt: now + 3 * 86_400_000 + 1000 }] }] }
  assert.equal(planLimits(near, 'claude', null, now)[0].level, 'warn', 'within a quarter of the allowance')
})

test('the session turns amber from 70% and red from 90%', () => {
  const level = (percent: number) => planLimits({ providers: [{ ...usage.providers[0], windows: [{ id: 'f', label: '5-hour', percent }] }] }, 'claude', null, now)[0].level
  assert.deepEqual([level(10), level(70), level(89), level(90)], ['ok', 'warn', 'warn', 'over'])
})
