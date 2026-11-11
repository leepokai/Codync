import assert from 'node:assert/strict'
import { test } from 'node:test'
import { spentNote, spentWindow } from './usage-presentation.ts'

const now = new Date('2026-11-08T12:00:00Z')
const at = (minutes: number) => now.getTime() + minutes * 60_000
const window = (id: string, percent: number, resetsAt: number | null = at(60), text?: string) =>
  ({ id, label: id, percent, resetsAt, text })
const usage = (...providers: { id: string; source: string; windows: ReturnType<typeof window>[] }[]) => ({
  providers: providers.map((p) => ({ ...p, name: p.id, updatedAt: 1 })),
})

test("a spent window stops its own agent's models", () => {
  const report = usage(
    { id: 'claude:work', source: 'claude', windows: [window('five_hour', 100)] },
    { id: 'codex', source: 'sessions', windows: [window('primary', 40)] },
  )
  assert.equal(spentWindow(report, 'claude', now)?.id, 'five_hour')
  assert.equal(spentWindow(report, 'codex', now), null)
  assert.equal(spentWindow(report, 'gemini', now), null)
})

test('the provider source names the agent too', () => {
  const report = usage({ id: 'antigravity', source: 'antigravity', windows: [window('weekly', 120)] })
  assert.equal(spentWindow(report, 'antigravity', now)?.id, 'weekly')
})

test('an unspent, a note or an already reset window is not a stop', () => {
  const report = usage({
    id: 'codex',
    source: 'sessions',
    windows: [window('primary', 99), window('secondary', 100, at(-1)), window('tokens', 100, null, '1.2M tokens')],
  })
  assert.equal(spentWindow(report, 'codex', now), null)
})

test('the note carries the reset time of the fullest spent window', () => {
  const report = usage({ id: 'claude', source: 'claude', windows: [window('weekly', 100, at(240)), window('five_hour', 160, at(60))] })
  const tr = (text: string) => text
  assert.equal(spentNote(report, 'claude', tr, now), 'Usage limit reached · resets in 1h 0m')
  assert.equal(spentNote(usage(), 'claude', tr, now), null)
})
