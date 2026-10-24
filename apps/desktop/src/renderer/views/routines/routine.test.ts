import assert from 'node:assert/strict'
import { test } from 'node:test'
import { compiledCron, durationText, repeatChoice, runLabel, type RoutineRun, type RoutineSchedulePreview } from './routine.ts'
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
  for (const style of ['minutes', 'days', 'monthly', 'custom']) assert.equal(repeatChoice(style), 'custom')
})

test('Custom starts from the cron the host compiled', () => {
  const preview = (triggers: RoutineSchedulePreview['triggers']) => ({ triggers }) as RoutineSchedulePreview
  assert.equal(compiledCron(preview([{ type: 'cron', expression: '0 9 * * 1-5', timeZone: 'Asia/Taipei' }])), '0 9 * * 1-5')
  assert.equal(compiledCron(preview([{ type: 'webhook' }])), null)
  assert.equal(compiledCron(null), null)
})

test('a queued run says when a waiting retry goes', () => {
  const run = (notBefore: number | null): RoutineRun => ({ id: 'r', routineId: 'x', botId: 'b', status: 'pending', createdAt: 0, notBefore })
  assert.equal(runLabel(run(null)), 'Queued')
  assert.match(runLabel(run(120_000), 60_000), /^Retrying at \S/)
  assert.equal(runLabel(run(60_000), 120_000), 'Retrying')
  assert.equal(runLabel({ ...run(null), status: 'failed' }), 'Failed')
})
