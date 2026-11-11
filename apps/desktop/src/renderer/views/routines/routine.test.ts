import assert from 'node:assert/strict'
import { test } from 'node:test'
import { compiledCron, durationText, emptyDraft, outputProblem, repeatChoice, runLabel, type RoutineRun, type RoutineSchedulePreview } from './routine.ts'
import { matchZones, timeZoneOptions, zoneOffset } from './time-zones.ts'

test('time zones match like the phones: name start, word starts, inside, letters in order', () => {
  const zones = timeZoneOptions(new Date('2026-01-15T00:00:00Z'))
  const ids = (query: string) => matchZones(zones, query).map((z) => z.id)
  assert.equal(ids('taipei')[0], 'Asia/Taipei')
  assert.equal(ids('new york')[0], 'America/New_York')
  assert.equal(ids('asia tai')[0], 'Asia/Taipei')
  assert.ok(ids('aipe').includes('Asia/Taipei'))
  assert.ok(ids('tpe').includes('Asia/Taipei'))
  // Name start, then word starts, then inside a word, then letters in order; ties keep list order.
  const few = [{ id: 'America/New_York', offset: '' }, { id: 'Asia/Taipei', offset: '' }, { id: 'Asia/Tokyo', offset: '' }, { id: 'America/Argentina/Buenos_Aires', offset: '' }]
  assert.deepEqual(matchZones(few, 'a').map((z) => z.id), ['America/New_York', 'Asia/Taipei', 'Asia/Tokyo', 'America/Argentina/Buenos_Aires'])
  assert.deepEqual(matchZones(few, 'ai').map((z) => z.id), ['America/Argentina/Buenos_Aires', 'Asia/Taipei', 'America/New_York', 'Asia/Tokyo'])
  assert.deepEqual(matchZones(few, 'nyk').map((z) => z.id), ['America/New_York'])
  assert.ok(ids('utc').includes('UTC'))
  assert.deepEqual(ids('nowhere-at-all'), [])
  // ICU's retired names are listed by their current ones.
  assert.equal(ids('kolkata')[0], 'Asia/Kolkata')
  assert.equal(ids('kyiv')[0], 'Europe/Kyiv')
  assert.ok(!zones.some((z) => z.id === 'Asia/Calcutta'))
  assert.equal(ids('').length, zones.length)
})

test('a zone shows its offset right now', () => {
  assert.equal(zoneOffset('Asia/Taipei', new Date('2026-01-15T00:00:00Z')), 'GMT+8')
  assert.equal(zoneOffset('America/New_York', new Date('2026-07-15T00:00:00Z')), 'GMT-4')
  assert.equal(zoneOffset('Not/A_Zone'), '')
})

test('a timeout reads in the largest whole unit', () => {
  assert.equal(durationText('3600'), '1 hour')
  assert.equal(durationText('7200'), '2 hours')
  assert.equal(durationText('1800'), '30 minutes')
  assert.equal(durationText('60'), '1 minute')
  assert.equal(durationText('90'), '90 seconds')
  assert.equal(durationText('1'), '1 second')
  assert.equal(durationText('abc'), 'abcs')
  assert.equal(durationText('0'), '0s')
})

test('styles the editor has no choice for open as Custom', () => {
  assert.equal(repeatChoice('weekdays'), 'weekdays')
  assert.equal(repeatChoice('hourly'), 'hourly')
  assert.equal(repeatChoice('yearly'), 'yearly')
  for (const style of ['minutes', 'days', 'monthly', 'custom']) assert.equal(repeatChoice(style), 'custom')
})

test('Custom starts from the cron the host compiled', () => {
  const preview = (triggers: RoutineSchedulePreview['triggers']) => ({ triggers }) as RoutineSchedulePreview
  assert.equal(compiledCron(preview([{ type: 'cron', expression: '0 9 * * 1-5', timeZone: 'Asia/Taipei' }])), '0 9 * * 1-5')
  assert.equal(compiledCron(preview([{ type: 'webhook' }])), null)
  assert.equal(compiledCron(null), null)
})

test('an event draft starts at a valid GitHub event with no filters', () => {
  const draft = emptyDraft()
  assert.equal(draft.eventSource, 'github')
  assert.equal(draft.eventName, 'issues.opened')
  assert.deepEqual(draft.eventFilters, {})
  assert.equal(draft.kind, 'cron', 'a new routine still starts on a schedule')
})

test('an output checks its own destination', () => {
  const output = (kind: string, target: string) => ({ kind, target })
  assert.equal(outputProblem(output('slack', 'C0123')), null)
  assert.equal(outputProblem(output('telegram', '-100123')), null)
  assert.ok(outputProblem(output('whatsapp', '5215555555555')))
  assert.equal(outputProblem(output('githubComment', 'owner/repo')), null)
  assert.ok(outputProblem(output('slack', 'not a channel')))
  assert.ok(outputProblem(output('telegram', 'abc')))
  assert.ok(outputProblem(output('githubComment', 'owner')))
  assert.ok(outputProblem(output('discordReact', 'two words')))
  assert.equal(outputProblem(output('discordReact', '👍')), null)
  assert.ok(outputProblem(output('slack', '')))
  assert.ok(outputProblem(output('whatsapp', '5215555555555@s.whatsapp.net')))
  assert.equal(outputProblem({ kind: 'whatsapp', target: '5215555555555@s.whatsapp.net', account: '5215555555556@s.whatsapp.net' }), null)
})

test('a queued run says when a waiting retry goes', () => {
  const run = (notBefore: number | null): RoutineRun => ({ id: 'r', routineId: 'x', botId: 'b', status: 'pending', createdAt: 0, notBefore })
  assert.equal(runLabel(run(null)), 'Queued')
  assert.match(runLabel(run(120_000), 60_000), /^Retrying at \S/)
  assert.equal(runLabel(run(60_000), 120_000), 'Retrying')
  assert.equal(runLabel({ ...run(null), status: 'failed' }), 'Failed')
})

test('a run says what it stood for: skipped, missed, or waiting on the user', () => {
  const run = (patch: Partial<RoutineRun>): RoutineRun => ({
    id: 'r',
    routineId: 'x',
    botId: 'b',
    status: 'running',
    createdAt: 0,
    ...patch,
  })
  assert.equal(runLabel(run({})), 'Running')
  assert.equal(runLabel(run({ needsAttention: true })), 'Waiting on you')
  // A cancelled run that stands for a due occurrence is a skipped one, and says how many it dropped.
  assert.equal(runLabel(run({ status: 'cancelled', scheduledFor: 1 })), 'Skipped')
  assert.equal(runLabel(run({ status: 'cancelled', scheduledFor: 1, missed: { count: 2, until: 2 } })), 'Skipped · 2 missed')
  assert.equal(runLabel(run({ status: 'cancelled', missed: { count: 3 } })), 'Skipped · 3 missed')
  // A cancelled run with no occurrence just stopped.
  assert.equal(runLabel(run({ status: 'cancelled' })), 'Cancelled')
  // A failure that also dropped occurrences keeps both facts.
  assert.equal(runLabel(run({ status: 'failed', missed: { count: 1 } })), 'Failed · 1 missed')
})
