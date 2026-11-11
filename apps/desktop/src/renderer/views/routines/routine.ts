import { trNow } from '../../lib/l10n.ts'

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
  /** `watch` triggers: `files` or `url`, the path or URL, and how often it's checked. */
  kind?: string | null
  target?: string | null
  everyMinutes?: number | null
}

export interface RoutineOutput {
  /** `slack`, `telegram`, `discord` or `githubComment`. */
  kind: string
  /** A channel id, a chat id, or the repository (`owner/name`). */
  target: string
  account?: string | null
  /** Labels, reviewers, or the issue title. */
  value?: string | null
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
  nextRuns?: number[] | null
  timeoutSeconds?: number | null
  lastError?: string | null
  /** What a due occurrence that cannot run follows: `runOnce` or `skip`. */
  missedPolicy?: string | null
  /** What a run that would pass the bot's daily token limit follows: `wait` or `skip`. */
  limitPolicy?: string | null
  /** Where a workflow's last reply goes. */
  outputs?: RoutineOutput[] | null
  outputDescriptions?: string[] | null
  /** Runs post in a thread of the bot's chat. */
  thread?: boolean | null
}

export interface RoutineRun {
  id: string
  routineId: string
  botId: string
  status: string
  createdAt: number
  finishedAt?: number | null
  detail?: string | null
  /** A stable failure reason (`timeout`, `usage_limit`, `memory`…), for scanning the history. */
  failureCode?: string | null
  /** A retry of a run that failed before acting waits until then. */
  notBefore?: number | null
  /** The occurrence this run stands for, when its schedule named one. */
  scheduledFor?: number | null
  /** Occurrences that came due and did not run; `missed.count` is how many. */
  missed?: { count: number; until?: number | null } | null
  /** A run holds the bot's turn and it is asking a question: it waits on the user. */
  needsAttention?: boolean | null
}

export const isActiveRun = (run: RoutineRun) => ['pending', 'starting', 'running', 'recovering'].includes(run.status)

/** A run's status in the routines list; a waiting retry says when it goes, a skipped run why. */
/** What a failure's stable code reads as; unknown codes fall back to a plain error. */
export function failureCodeLabel(code: string | null | undefined) {
  switch (code) {
    case 'timeout':
      return 'Timed out'
    case 'usage_limit':
      return 'Usage limit'
    case 'memory':
      return 'Out of memory'
    case 'denied':
      return 'Denied'
    case 'busy':
      return 'Bot busy'
    case 'missing':
      return 'Missing file or tool'
    case 'stopped':
      return 'Stopped'
    default:
      return 'Error'
  }
}

export function runLabel(run: RoutineRun, now = Date.now()) {
  const missed = run.missed && run.missed.count > 0 ? ` · ${run.missed.count} missed` : ''
  switch (run.status) {
    case 'pending':
      if (run.notBefore && run.notBefore > now) {
        const at = new Date(run.notBefore).toLocaleTimeString(undefined, { timeStyle: 'short' })
        return trNow('Retrying at {time}').replaceAll('{time}', () => at)
      }
      return run.notBefore ? trNow('Retrying') : trNow('Queued')
    case 'starting':
      return trNow('Starting')
    case 'running':
      return run.needsAttention ? trNow('Waiting on you') : trNow('Running')
    case 'recovering':
      return trNow('Resuming after restart')
    case 'failed':
      return `${trNow('Failed')}${missed}`
    case 'interrupted':
      return `${trNow('Interrupted')}${missed}`
    case 'cancelled':
      // A cancelled run that stands for a due occurrence is a skipped one; the rest just stopped.
      return run.scheduledFor || run.missed ? `${trNow('Skipped')}${missed}` : trNow('Cancelled')
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

/** A moment on the routines calendar: a run that stood for it, or an occurrence still to come. */
export interface RoutineCalendarMoment {
  routineId: string
  at: number
}

export interface RoutineCalendarRun extends RoutineCalendarMoment {
  id: string
  status: string
  /** It stands for a due occurrence that did not run; a cancelled run that only stopped isn't. */
  skipped?: boolean | null
}

export interface RoutineCalendarListing {
  routines: Routine[]
  runs: RoutineCalendarRun[]
  scheduled: RoutineCalendarMoment[]
}

/** Form values only. */
export interface RoutineScheduleDraft {
  kind: string
  amount: string
  unit: number
  calendarStyle: string
  weekday: number
  /** Hourly: the weekdays it runs on; null or empty is every day. */
  selectedDays: number[] | null
  monthDay: number
  /** Yearly: the month (1-12) of the one date. */
  yearMonth: number
  minuteStep: number
  /** Hourly: repeat every N hours (1, 2, 3, 4, 6, 8, 12); 1 runs on the hour. */
  everyHours: number
  /** Hourly: the window, minutes from midnight; null on either end runs through the day. */
  windowStart: number | null
  windowEnd: number | null
  hour: number
  minute: number
  at: number
  expression: string
  zone: string
  /** An event trigger: the service, the event it sends and its filters. */
  eventSource: string
  eventName: string
  eventFilters: Record<string, string>
  original: RoutineTrigger[]
}

export const emptyDraft = (): RoutineScheduleDraft => ({
  kind: 'cron',
  amount: '1',
  unit: 3600,
  calendarStyle: 'daily',
  weekday: 1,
  selectedDays: null,
  monthDay: 1,
  yearMonth: 1,
  minuteStep: 15,
  everyHours: 1,
  windowStart: null,
  windowEnd: null,
  hour: 9,
  minute: 0,
  at: 0,
  expression: '',
  zone: '',
  eventSource: 'github',
  eventName: 'issues.opened',
  eventFilters: {},
  original: [],
})

export const sameDraft = (a: RoutineScheduleDraft | null, b: RoutineScheduleDraft | null) => !!a && !!b && JSON.stringify(a) === JSON.stringify(b)

export interface RoutineEventSource {
  id: string
  label: string
  events: { id: string; label: string }[]
  filters: { key: string; label: string; hint: string }[]
}

export interface RoutineSchedulePreview {
  draft: RoutineScheduleDraft
  triggers: RoutineTrigger[]
  summary: string
  nextRunAt?: number | null
  nextRuns?: number[] | null
  warning?: string | null
  /** The event services and filters the editor offers. */
  sources?: RoutineEventSource[]
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
  { id: 'yearly', label: 'Yearly' },
  { id: 'custom', label: 'Custom' },
] as const

export type RepeatChoice = (typeof repeatChoices)[number]['id']

export const repeatChoice = (style: string): RepeatChoice => repeatChoices.find((c) => c.id === style)?.id ?? 'custom'

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

/** The output kinds the editor offers. */
export const outputKinds = [
  { id: 'slack', label: 'Slack' },
  { id: 'telegram', label: 'Telegram' },
  { id: 'whatsapp', label: 'WhatsApp' },
  { id: 'discord', label: 'Discord' },
  { id: 'discordReact', label: 'Discord reaction' },
  { id: 'githubComment', label: 'GitHub comment' },
  { id: 'githubIssue', label: 'GitHub issue' },
  { id: 'githubLabel', label: 'GitHub label' },
  { id: 'githubAssign', label: 'GitHub assign' },
  { id: 'githubReview', label: 'Request review' },
] as const

/** Why one output can't be saved, or null when it can; the same rules as the host's. */
export function outputProblem(output: RoutineOutput): string | null {
  const target = output.target.trim()
  if (!target) return 'An output needs a destination.'
  if (output.kind === 'whatsapp' && !output.account?.trim()) return 'Use the linked WhatsApp account id as account.'
  if (output.kind === 'telegram' && !/^-?\d+$/.test(target)) return 'Use a Telegram chat id.'
  if (output.kind === 'whatsapp' && !/^[\d-]+@(s\.whatsapp\.net|g\.us)$/.test(target)) return 'Use a WhatsApp chat id.'
  if (output.kind === 'githubComment' && !/^[^/\s]+\/[^/\s]+$/.test(target)) return 'Use owner/name.'
  if ((output.kind === 'slack' || output.kind === 'discord') && /\s/.test(target)) return 'Use a channel id.'
  if (output.kind === 'discordReact' && /\s/.test(target)) return 'Use one emoji or name:id.'
  if (['githubLabel', 'githubAssign', 'githubReview'].includes(output.kind) && !output.value?.trim()) return 'This output needs a value.'
  return null
}
