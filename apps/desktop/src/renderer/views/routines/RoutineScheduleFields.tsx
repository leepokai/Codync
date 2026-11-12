import type { ReactNode } from 'react'
import { CardSection, ChipChoice, ChoicePicker } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { TimeZonePicker } from './TimeZonePicker'
import { compiledCron, repeatChoice, repeatChoices, type RepeatChoice, type RoutineScheduleDraft, type RoutineSchedulePreview } from './routine'
import './routines.css'
import { useIntlLocale, useTr } from '../../lib/use-language'


const weekdays = [
  { id: 1, label: 'Mon', title: 'Monday' },
  { id: 2, label: 'Tue', title: 'Tuesday' },
  { id: 3, label: 'Wed', title: 'Wednesday' },
  { id: 4, label: 'Thu', title: 'Thursday' },
  { id: 5, label: 'Fri', title: 'Friday' },
  { id: 6, label: 'Sat', title: 'Saturday' },
  { id: 0, label: 'Sun', title: 'Sunday' },
]
const pad = (n: number) => String(n).padStart(2, '0')
const minutes = Array.from({ length: 60 }, (_, m) => ({ id: m, label: `:${pad(m)}` }))

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
          <ChipChoice label={tr('Day')} selection={schedule.weekday} options={weekdays} onChange={(weekday) => update({ weekday })} />
        </LabeledRow>
      ) : null}
      {repeat === 'hourly' ? (
        <LabeledRow key="At minute" label="At minute">
          <ChoicePicker selection={schedule.minute} options={minutes} onChange={(minute) => update({ minute })} fill="var(--field)" />
        </LabeledRow>
      ) : repeat === 'custom' ? (
        <LabeledRow key="Cron" label="Cron" top>
          <VStack gap={6} grow>
            <input
              className="schedule-input routine-cron-input"
              aria-label={tr('Cron')}
              value={schedule.expression}
              placeholder="0 9 * * 1-5"
              spellCheck={false}
              autoCorrect="off"
              onChange={(e) => update({ expression: e.target.value })}
            />
            <HStack gap={16} wrap className="routine-cron-hint">
              {tr('minute  hour  day  month  weekday').split(/\s{2,}/).map((word) => (
                <Text key={word} size="caption" mono tone="tertiary">{word}</Text>
              ))}
            </HStack>
          </VStack>
        </LabeledRow>
      ) : (
        <LabeledRow key="At" label="At">
          <input
            type="time"
            className="schedule-input schedule-time"
            aria-label={tr('Time')}
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
  const tr = useTr()
  return (
    <HStack align={top ? 'flex-start' : 'center'} className={`labeled-row${top ? ' top' : ''}${className ? ` ${className}` : ''}`}>
      <Text size="callout" weight="medium" tone="secondary" className="labeled-row-label">{tr(label)}</Text>
      {children}
    </HStack>
  )
}

/** The sentence, then the next run, under the group — or why the host can't read it. */
function SchedulePreview({ preview, error }: { preview: RoutineSchedulePreview | null; error: string | null }) {
  const tr = useTr()
  const dateLocale = useIntlLocale()
  if (error) return <Text size="callout" tone="danger" className="schedule-preview">{error}</Text>
  const nextRunAt = preview?.nextRunAt
  return (
    <VStack gap={8} className="schedule-preview" aria-live="polite">
      {preview ? (
        <>
          <Text size="callout" weight="medium" tone="text">{tr(preview.summary)}</Text>
          {nextRunAt ? (
            <Text size="callout" tone="secondary">
              {tr('Next {when}').replaceAll('{when}', () => new Date(nextRunAt).toLocaleString(dateLocale, { dateStyle: 'medium', timeStyle: 'short' }))}
            </Text>
          ) : null}
          {preview.warning ? <Text size="callout" tone="secondary">{preview.warning}</Text> : null}
        </>
      ) : null}
    </VStack>
  )
}
