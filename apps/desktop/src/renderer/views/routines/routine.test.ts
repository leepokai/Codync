import assert from 'node:assert/strict'
import { test } from 'node:test'
import { translate } from '../../lib/l10n.ts'
import { compiledCron, durationText, emptyDraft, folderField, followUpLine, repeatChoice, runLabel, scheduleCaption, scheduleLine, schedulePreviewLine, type FollowUp, type Routine, type RoutineRun, type RoutineSchedulePreview } from './routine.ts'
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
  assert.equal(durationText('3600', 'it').replaceAll('\u00a0', ' ').replaceAll('\u202f', ' '), '1 ora')
})

test('styles the editor has no choice for open as Custom', () => {
  assert.equal(repeatChoice('weekdays'), 'weekdays')
  assert.equal(repeatChoice('hourly'), 'hourly')
  for (const style of ['minutes', 'days', 'monthly', 'custom']) assert.equal(repeatChoice(style), 'custom')
})

test('the editor line uses the draft, so it can be translated', () => {
  const tr = (key: string) => ({ Hourly: '每小时', Daily: '每天', Weekdays: '工作日', Weekly: '每周', Mon: '周一' })[key] ?? key
  const base = { ...emptyDraft(), zone: 'Asia/Shanghai', hour: 9, minute: 0 }
  assert.equal(scheduleCaption({ ...base, calendarStyle: 'weekly', weekday: 1 }, tr), '每周 · 周一 · 09:00 · Asia/Shanghai')
  assert.equal(scheduleCaption({ ...base, calendarStyle: 'daily' }, tr), '每天 · 09:00 · Asia/Shanghai')
  assert.equal(scheduleCaption({ ...base, calendarStyle: 'hourly', minute: 5 }, tr), '每小时 · :05 · Asia/Shanghai')
  assert.equal(scheduleCaption({ ...base, calendarStyle: 'weekdays' }, tr), '工作日 · 09:00 · Asia/Shanghai')
  assert.equal(scheduleCaption({ ...base, calendarStyle: 'custom', expression: '0 9 * * 1' }, tr), '0 9 * * 1 · Asia/Shanghai')
  assert.equal(scheduleCaption({ ...base, calendarStyle: 'weekly', weekday: 9 }, tr), '每周 · 周一 · 09:00 · Asia/Shanghai')
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

test('a waiting follow-up says when it happens: the time today, the date too on another day, or that it is due', () => {
  const now = new Date(2026, 10, 14, 15, 0).getTime()
  const followUp = (at: number): FollowUp => ({ id: 'f', botId: 'b', at, note: 'Check the build', createdAt: now })
  // The app language's way of writing a time, whatever the computer's is.
  assert.equal(followUpLine(followUp(now + 20 * 60_000), 'en-GB', now), 'Follow-up at 15:20')
  const tomorrow = now + 24 * 3_600_000
  const dated = new Date(tomorrow).toLocaleString('zh-Hant', { dateStyle: 'medium', timeStyle: 'short' })
  assert.equal(followUpLine(followUp(tomorrow), 'zh-Hant', now), `Follow-up at ${dated}`)
  // Due, and waiting its turn behind what the bot is doing.
  assert.equal(followUpLine(followUp(now - 1), 'en-GB', now), 'Follow-up due now')
})

test("saving sends the routine's folder on a host that has it: the path, or null for the bot's own", () => {
  assert.deepEqual(folderField('/Users/me/repo', true), { folder: '/Users/me/repo' })
  assert.deepEqual(folderField(null, true), { folder: null })
  assert.deepEqual(folderField('', true), { folder: null })
  // An older host never hears of it.
  assert.deepEqual(folderField('/Users/me/repo', false), {})
})

test('triggerPresentation renders every trigger kind in the app language and falls back cleanly for older hosts', () => {
  const zh = (text: string) => translate(text, 'zh', false)
  const en = (text: string) => translate(text, 'en', false)
  const baseRoutine = (overrides: Partial<Routine>): Routine => ({
    id: 'r1',
    botId: 'b1',
    name: 'Check',
    instruction: 'Run',
    triggers: [],
    enabled: true,
    createdAt: 0,
    updatedAt: 0,
    triggerDescriptions: [],
    ...overrides,
  })

  // Interval, calendar (daily / weekdays / single weekday), webhook, event.
  const multi = baseRoutine({
    triggerDescriptions: ['Every 30 minutes', 'Every weekday at 09:00 · Asia/Taipei', 'Every Sunday at 14:20 · Asia/Taipei', 'When a webhook fires', 'github · pull_request.opened'],
    triggerPresentation: {
      version: 1,
      items: [
        { kind: 'interval', seconds: 1800 },
        { kind: 'calendar', weekdays: ['Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday'], hour: 9, minute: 0, expression: '0 9 * * 1-5', timeZone: 'Asia/Taipei' },
        { kind: 'calendar', weekdays: ['Sunday'], hour: 14, minute: 20, expression: '20 14 * * 0', timeZone: 'Asia/Taipei' },
        { kind: 'webhook' },
        { kind: 'event', source: 'github', event: 'pull_request.opened' },
      ],
    },
  })
  assert.equal(
    scheduleLine(multi, zh, 'zh-Hans'),
    '每 30 分钟 · 工作日 09:00 · Asia/Taipei · 每周日 14:20 · Asia/Taipei · 收到 Webhook 时 · 当 github 触发 pull_request.opened',
  )

  // Once uses the localized date/time format.
  const at = Date.UTC(2026, 10, 23, 9, 30)
  const formatted = new Date(at).toLocaleString('zh-Hans', { dateStyle: 'medium', timeStyle: 'short' })
  assert.equal(
    scheduleLine(baseRoutine({
      triggerDescriptions: ['Once at 2026-11-23T09:30:00Z'],
      triggerPresentation: { version: 1, items: [{ kind: 'once', at }] },
    }), zh, 'zh-Hans'),
    `一次：${formatted}`,
  )

  // Raw cron shapes: host calendar phrases translate via catalog; raw croner English keeps expression + zone in non-English UI.
  assert.equal(
    scheduleLine(baseRoutine({
      triggerDescriptions: ['Every hour at minute 47 · Europe/Rome', 'Every 15 minutes · Europe/Rome'],
      triggerPresentation: {
        version: 1,
        items: [
          { kind: 'cron', expression: '47 * * * *', timeZone: 'Europe/Rome' },
          { kind: 'cron', expression: '*/15 * * * *', timeZone: 'Europe/Rome' },
        ],
      },
    }), zh, 'zh-Hans'),
    '每小时第 47 分钟 · Europe/Rome · */15 * * * * · Europe/Rome',
  )
  assert.equal(
    scheduleLine(baseRoutine({
      triggerDescriptions: ['Every 15 minutes · Europe/Rome'],
      triggerPresentation: { version: 1, items: [{ kind: 'cron', expression: '*/15 * * * *', timeZone: 'Europe/Rome' }] },
    }), en, 'en'),
    'Every 15 minutes · Europe/Rome',
  )

  // Legacy host (no triggerPresentation) or unknown presentation version/kind falls back to triggerDescriptions.
  assert.equal(
    scheduleLine(baseRoutine({ triggerDescriptions: ['Every 10 minutes', 'When a webhook fires'] }), zh, 'zh-Hans'),
    '每 10 分钟 · 收到 Webhook 时',
  )
  assert.equal(
    scheduleLine(baseRoutine({
      triggerDescriptions: ['Every day at 09:00 · UTC'],
      triggerPresentation: { version: 99, items: [{ kind: 'future' }] },
    }), zh, 'zh-Hans'),
    '每天 09:00 · UTC',
  )

  // Preview helper matches routine scheduleLine without splitting single-trigger descriptions on ' · '.
  const preview = {
    triggers: [],
    summary: 'Every hour at minute 47 · Europe/Rome',
    triggerPresentation: { version: 1, items: [{ kind: 'cron', expression: '47 * * * *', timeZone: 'Europe/Rome' }] },
  } as unknown as RoutineSchedulePreview
  assert.equal(schedulePreviewLine(preview, zh, 'zh-Hans'), '每小时第 47 分钟 · Europe/Rome')
})
