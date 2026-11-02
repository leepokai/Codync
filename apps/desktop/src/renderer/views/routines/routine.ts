// Routines (kit Routine.swift, RoutineScheduleDraft.swift). The host hydrates, compiles,
// validates and describes schedules; the app only keeps form values.

export interface RoutineTrigger {
  type: string
  seconds?: number | null
  at?: number | null
  expression?: string | null
  timeZone?: string | null
  source?: string | null
  event?: string | null
  filters?: Record<string, string> | null
}

export interface Routine {
  id: string
  botId: string
  name: string
  instruction: string
  triggers: RoutineTrigger[]
  enabled: boolean
  createdAt: number
  updatedAt: number
  triggerDescriptions: string[]
  nextRunAt?: number | null
  timeoutSeconds?: number | null
  lastError?: string | null
}

export interface RoutineRun {
  id: string
  routineId: string
  botId: string
  status: string
  createdAt: number
  finishedAt?: number | null
  detail?: string | null
  /** A retry of a run that failed before acting waits until then. */
  notBefore?: number | null
}

export const isActiveRun = (run: RoutineRun) => ['pending', 'starting', 'running', 'recovering'].includes(run.status)

/** A run's status in the routines list; a waiting retry says when it goes. */
export function runLabel(run: RoutineRun, now = Date.now()) {
  switch (run.status) {
    case 'pending':
      if (run.notBefore && run.notBefore > now) return `Retrying at ${new Date(run.notBefore).toLocaleTimeString(undefined, { timeStyle: 'short' })}`
      return run.notBefore ? 'Retrying' : 'Queued'
    case 'starting':
      return 'Starting'
    case 'running':
      return 'Running'
    case 'recovering':
      return 'Resuming after restart'
    case 'failed':
      return 'Failed'
    case 'interrupted':
      return 'Interrupted'
    default:
      return run.status.charAt(0).toUpperCase() + run.status.slice(1)
  }
}

export interface RoutineListing {
  routines: Routine[]
  runs: RoutineRun[]
}

/** `url` is public through the Codync cloud (null while the cloud is off), `localUrl` works on the host itself. */
export interface RoutineWebhook {
  url?: string | null
  localUrl: string
  key: string
  connected: boolean
}

/** Form values only. */
export interface RoutineScheduleDraft {
  kind: string
  amount: string
  unit: number
  calendarStyle: string
  weekday: number
  selectedDays: number[]
  monthDay: number
  minuteStep: number
  hour: number
  minute: number
  at: number
  expression: string
  zone: string
  original: RoutineTrigger[]
}

export const emptyDraft = (): RoutineScheduleDraft => ({
  kind: 'cron',
  amount: '1',
  unit: 3600,
  calendarStyle: 'daily',
  weekday: 1,
  selectedDays: [],
  monthDay: 1,
  minuteStep: 15,
  hour: 9,
  minute: 0,
  at: 0,
  expression: '',
  zone: '',
  original: [],
})

export const sameDraft = (a: RoutineScheduleDraft | null, b: RoutineScheduleDraft | null) => !!a && !!b && JSON.stringify(a) === JSON.stringify(b)

export interface RoutineSchedulePreview {
  draft: RoutineScheduleDraft
  triggers: RoutineTrigger[]
  summary: string
  nextRunAt?: number | null
  warning?: string | null
}

/**
 * The Repeat choices: the host's calendar styles the editor offers. A schedule in any other
 * style (every few minutes, several days, monthly) opens as Custom, its cron as written.
 */
export const repeatChoices = [
  { id: 'hourly', label: 'Hourly' },
  { id: 'daily', label: 'Daily' },
  { id: 'weekdays', label: 'Weekdays' },
  { id: 'weekly', label: 'Weekly' },
  { id: 'custom', label: 'Custom' },
] as const

export type RepeatChoice = (typeof repeatChoices)[number]['id']

export const repeatChoice = (style: string): RepeatChoice => repeatChoices.find((c) => c.id === style)?.id ?? 'custom'

const weekdayKey = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'] as const
const twoDigits = (n: number) => String(n).padStart(2, '0')

/**
 * The line under the schedule fields. The host's cron sentence is English, so this is
 * built from the draft and translated with the same words as the fields above it.
 */
export function scheduleCaption(schedule: RoutineScheduleDraft, tr: (key: string) => string): string {
  const time = `${twoDigits(schedule.hour)}:${twoDigits(schedule.minute)}`
  const zone = schedule.zone
  switch (schedule.calendarStyle) {
    case 'hourly':
      return `${tr('Hourly')} · :${twoDigits(schedule.minute)} · ${zone}`
    case 'daily':
      return `${tr('Daily')} · ${time} · ${zone}`
    case 'weekdays':
      return `${tr('Weekdays')} · ${time} · ${zone}`
    case 'weekly':
      return `${tr('Weekly')} · ${tr(weekdayKey[schedule.weekday] ?? 'Mon')} · ${time} · ${zone}`
    default: {
      const expression = schedule.expression.trim()
      return expression ? `${expression} · ${zone}` : zone
    }
  }
}

/** The cron the host compiled for a checked schedule, to start Custom from the same times. */
export const compiledCron = (preview: RoutineSchedulePreview | null) => {
  const [trigger] = preview?.triggers ?? []
  return trigger?.type === 'cron' ? (trigger.expression ?? null) : null
}

/** A timeout in seconds as people say it ("1 hour", "30 minutes", "90 seconds"); other text as typed. */
export function durationText(text: string) {
  const value = text.trim()
  if (!/^\d+$/.test(value) || Number(value) === 0) return `${value}s`
  const seconds = Number(value)
  const count = (amount: number, unit: string) => `${amount} ${unit}${amount === 1 ? '' : 's'}`
  if (seconds % 3600 === 0) return count(seconds / 3600, 'hour')
  if (seconds % 60 === 0) return count(seconds / 60, 'minute')
  return count(seconds, 'second')
}

export const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e))
