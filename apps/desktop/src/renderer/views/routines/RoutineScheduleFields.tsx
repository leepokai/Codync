import type { ReactNode } from 'react'
import { Button, CardSection, ChipChoice, ChoicePicker } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { TimeZonePicker } from './TimeZonePicker'
import { compiledCron, repeatChoice, repeatChoices, type RepeatChoice, type RoutineScheduleDraft, type RoutineSchedulePreview } from './routine'
import './routines.css'
import { useTr } from '../../lib/use-language'


const weekdays = [
  { id: 1, label: 'Mon', title: 'Monday' },
  { id: 2, label: 'Tue', title: 'Tuesday' },
  { id: 3, label: 'Wed', title: 'Wednesday' },
  { id: 4, label: 'Thu', title: 'Thursday' },
  { id: 5, label: 'Fri', title: 'Friday' },
  { id: 6, label: 'Sat', title: 'Saturday' },
  { id: 0, label: 'Sun', title: 'Sunday' },
]
const months = [
  { id: 1, label: 'Jan' }, { id: 2, label: 'Feb' }, { id: 3, label: 'Mar' }, { id: 4, label: 'Apr' },
  { id: 5, label: 'May' }, { id: 6, label: 'Jun' }, { id: 7, label: 'Jul' }, { id: 8, label: 'Aug' },
  { id: 9, label: 'Sep' }, { id: 10, label: 'Oct' }, { id: 11, label: 'Nov' }, { id: 12, label: 'Dec' },
]
/// The steps an hourly run repeats at; the host accepts exactly these.
const everyHours = [
  { id: 1, label: 'hour' }, { id: 2, label: '2 hours' }, { id: 3, label: '3 hours' }, { id: 4, label: '4 hours' },
  { id: 6, label: '6 hours' }, { id: 8, label: '8 hours' }, { id: 12, label: '12 hours' },
]
const pad = (n: number) => String(n).padStart(2, '0')
const minutes = Array.from({ length: 60 }, (_, m) => ({ id: m, label: `:${pad(m)}` }))
/// A leap year, so February 29 is a date, like the host counts it.
const daysIn = (month: number) => new Date(2024, month, 0).getDate()
/// `HH:MM` a window's minute of the day, as a time input shows it; `''` runs through the day.
const windowTime = (value: number | null) => (value === null ? '' : `${pad(Math.floor(value / 60))}:${pad(value % 60)}`)
const fromWindowTime = (value: string): number | null => {
  if (!value) return null
  const [hour, minute] = value.split(':').map(Number)
  return Number.isNaN(hour) || Number.isNaN(minute) ? null : hour * 60 + minute
}

/**
 * A schedule's fields in one group (how often, the day and time, the time zone), then what the
 * host read from them. The host compiles the cron; Custom edits it directly.
 */
export function RoutineScheduleFields({ schedule, update, preview, error }: {
  schedule: RoutineScheduleDraft
  update: (change: Partial<RoutineScheduleDraft>) => void
  /** The host's reading of exactly these fields, once it has checked them. */
  preview: RoutineSchedulePreview | null
  error: string | null
}) {
  const tr = useTr()
  const repeat = repeatChoice(schedule.calendarStyle)
  // Custom starts from the host's cron for the choice it replaces; empty while that check is
  // still out, never the cron the routine opened with.
  const choose = (id: RepeatChoice) => {
    if (id !== repeat) update(id === 'custom' ? { calendarStyle: id, expression: compiledCron(preview) ?? '' } : { calendarStyle: id })
  }

  return (
    <CardSection footer={<SchedulePreview preview={preview} error={error} />}>
      <LabeledRow key="Repeat" label="Repeat">
        <ChipChoice label="Repeat" selection={repeat} options={repeatChoices} onChange={choose} />
      </LabeledRow>
      {repeat === 'weekly' ? (
        <LabeledRow key="On" label="On">
          <ChipChoice label="Day" selection={schedule.weekday} options={weekdays} onChange={(weekday) => update({ weekday })} />
        </LabeledRow>
      ) : null}
      {repeat === 'yearly' ? (
        <LabeledRow key="On" label="On">
          <HStack gap={8}>
            <ChoicePicker
              selection={schedule.yearMonth}
              options={months}
              onChange={(yearMonth) => update({ yearMonth, monthDay: Math.min(schedule.monthDay, daysIn(yearMonth)) })}
              fits
            />
            <ChoicePicker selection={schedule.monthDay} options={Array.from({ length: daysIn(schedule.yearMonth) }, (_, d) => ({ id: d + 1, label: String(d + 1) }))} onChange={(monthDay) => update({ monthDay })} fits />
          </HStack>
        </LabeledRow>
      ) : null}
      {repeat === 'hourly' ? (
        <VStack key="Hourly" gap={10}>
          {schedule.windowStart === null && schedule.windowEnd === null ? (
            <LabeledRow label="At minute">
              <HStack gap={8}>
                <ChoicePicker selection={schedule.minute} options={minutes} onChange={(minute) => update({ minute })} fill="var(--field)" />
                {/* The window the TUI starts from: the workday, 09:00 to 18:00. */}
                <Button kind="secondary" title="Only run between two times" onClick={() => update({ windowStart: 9 * 60, windowEnd: 18 * 60 })}>
                  Window
                </Button>
              </HStack>
            </LabeledRow>
          ) : (
            // A window's start sets the minute of every run in it.
            <LabeledRow label="Window">
              <HStack gap={8}>
                <input
                  type="time"
                  className="schedule-input schedule-time"
                  aria-label="Window start"
                  value={windowTime(schedule.windowStart)}
                  onChange={(e) => update({ windowStart: fromWindowTime(e.target.value) })}
                />
                <Text size="callout" tone="secondary">to</Text>
                <input
                  type="time"
                  className="schedule-input schedule-time"
                  aria-label="Window end"
                  value={windowTime(schedule.windowEnd)}
                  onChange={(e) => update({ windowEnd: fromWindowTime(e.target.value) })}
                />
                <Button kind="secondary" title="Run through the whole day" onClick={() => update({ windowStart: null, windowEnd: null })}>
                  Off
                </Button>
              </HStack>
            </LabeledRow>
          )}
          <LabeledRow label="Every">
            <ChoicePicker selection={schedule.everyHours} options={everyHours} onChange={(everyHours) => update({ everyHours })} fits />
          </LabeledRow>
        </VStack>
      ) : repeat === 'custom' ? (
        <LabeledRow key="Cron" label="Cron" top>
          <VStack gap={6} grow>
            <input
              className="schedule-input routine-cron-input"
              aria-label="Cron"
              value={schedule.expression}
              placeholder="0 9 * * 1-5"
              spellCheck={false}
              autoCorrect="off"
              onChange={(e) => update({ expression: e.target.value })}
            />
            <Text size="caption" mono tone="tertiary" className="routine-cron-hint">{tr(`minute  hour  day  month  weekday`)}</Text>
          </VStack>
        </LabeledRow>
      ) : (
        <LabeledRow key="At" label="At">
          <input
            type="time"
            className="schedule-input schedule-time"
            aria-label="Time"
            value={`${pad(schedule.hour)}:${pad(schedule.minute)}`}
            onChange={(e) => {
              const [hour, minute] = e.target.value.split(':').map(Number)
              if (hour !== undefined && minute !== undefined && !Number.isNaN(hour) && !Number.isNaN(minute)) update({ hour, minute })
            }}
          />
        </LabeledRow>
      )}
      <LabeledRow key="Time zone" label="Time zone">
        <TimeZonePicker zone={schedule.zone} onChange={(zone) => update({ zone })} />
      </LabeledRow>
    </CardSection>
  )
}

/** A row of the routine editor's groups (`CardSection`): its label in a fixed column, then the field; `top` for a tall one. */
export function LabeledRow({ label, top = false, className, children }: { label: string; top?: boolean; className?: string; children: ReactNode }) {
  return (
    <HStack align={top ? 'flex-start' : 'center'} className={`labeled-row${top ? ' top' : ''}${className ? ` ${className}` : ''}`}>
      <Text size="callout" weight="medium" tone="secondary" className="labeled-row-label">{label}</Text>
      {children}
    </HStack>
  )
}

/** One quiet line under the group: what the host read, or why it can't. */
export function SchedulePreview({ preview, error }: { preview: RoutineSchedulePreview | null; error: string | null }) {
  const tr = useTr()
  if (error) return <Text size="callout" tone="danger" className="schedule-preview">{error}</Text>
  return (
    <VStack gap={2} className="schedule-preview" aria-live="polite">
      {preview ? (
        <>
          <HStack gap={8} wrap>
            <Text size="callout" weight="medium" tone="text">{preview.summary}</Text>
            {preview.nextRunAt ? (
              <Text size="callout" tone="secondary">
                {tr('Next')} {new Date(preview.nextRunAt).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' })}
              </Text>
            ) : null}
          </HStack>
          {preview.warning ? <Text size="callout" tone="secondary">{preview.warning}</Text> : null}
        </>
      ) : null}
    </VStack>
  )
}
