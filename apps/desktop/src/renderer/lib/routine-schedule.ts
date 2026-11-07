// Options of the visual routine schedule editor, mirroring the styles the host's schedule editor understands
// (host/src/routines/editor.rs). The host builds the cron expression, validates it and computes the next runs;
// this only lists what the editor can offer. Same options as the Android editor (core RoutineSchedule).

/** Trigger kinds the editor edits; any other (a one-off time, events, several triggers) keeps the original triggers. */
export const editableKinds = ['cron', 'interval', 'event', 'webhook']

/** Style → title, in display order. `custom` is the raw cron expression. */
export const scheduleStyles = [
  { id: 'minutes', label: 'Every few minutes' },
  { id: 'hourly', label: 'Hourly' },
  { id: 'daily', label: 'Daily' },
  { id: 'weekdays', label: 'Weekdays' },
  { id: 'weekly', label: 'Weekly' },
  { id: 'days', label: 'Chosen days' },
  { id: 'monthly', label: 'Monthly' },
  { id: 'yearly', label: 'Yearly' },
  { id: 'custom', label: 'Cron' },
]

/** Minute steps that divide the hour evenly; the host rejects others. */
export const minuteSteps = [1, 5, 10, 15, 20, 30]

/** Interval unit in seconds → title; the host accepts exactly these. */
export const intervalUnits = [
  { id: 1, label: 'Seconds' },
  { id: 60, label: 'Minutes' },
  { id: 3600, label: 'Hours' },
  { id: 86400, label: 'Days' },
]

/** Weekdays as the host numbers them: 0 Sunday … 6 Saturday. */
export const weekdayNames = ['Sunday', 'Monday', 'Tuesday', 'Wednesday', 'Thursday', 'Friday', 'Saturday']
export const weekdayInitials = ['S', 'M', 'T', 'W', 'T', 'F', 'S']

/** Styles whose cron has an hour as well as a minute. */
export const usesTime = (style: string) => ['daily', 'weekdays', 'weekly', 'days', 'monthly', 'yearly'].includes(style)

/** The kind the editor shows for what the host read: kinds it can't edit keep the original triggers. */
export const editorKind = (kind: string) => (editableKinds.includes(kind) ? kind : 'keep')

/** Toggle one weekday of the `days` style, keeping the list sorted; the host rejects an empty choice. */
export const toggleDay = (days: number[], day: number) =>
  (days.includes(day) ? days.filter((d) => d !== day) : [...days, day]).sort((a, b) => a - b)

const pad = (n: number) => String(n).padStart(2, '0')
export const wrap = (value: number, size: number) => ((value % size) + size) % size
/** `09:05`, as the time fields show it. */
export const clock = (hour: number, minute: number) => `${pad(wrap(hour, 24))}:${pad(wrap(minute, 60))}`
