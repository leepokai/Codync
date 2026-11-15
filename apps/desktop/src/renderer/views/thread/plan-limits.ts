// The plan limits of the login a bot runs on, shown above the message box like a terminal's status line: the
// 5-hour session and the week, from the usage the host reads on this computer (no model call).
// Plain functions, so they can be tested without a window.
import type { Usage, UsageWindow } from '../../../shared/models.ts'

export type LimitLevel = 'ok' | 'warn' | 'over'

export interface PlanLimit {
  id: string
  /** "Session", "Week", or the window's own name ("Opus"). */
  title: string
  percent: number
  /** Time left before it resets: "2h55", "45m", "3d". */
  reset: string | null
  /** The week's share a steady pace allows by now (day × 100/7); only on the week. */
  pace: number | null
  /** Day of the week, 1 to 7; only on the week. */
  day: number | null
  level: LimitLevel
}

const DAY = 86_400_000

/** "2h55", "45m", "3d" until `at`. */
export function untilShort(at: number, now: number): string {
  const s = Math.max(0, (at - now) / 1000)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h >= 48) return `${Math.floor(h / 24)}d`
  if (h > 0) return `${h}h${String(m).padStart(2, '0')}`
  return `${m}m`
}

const isWeek = (w: UsageWindow) => /^weekly$/i.test(w.label.trim())

function titleOf(w: UsageWindow): string {
  if (w.label === '5-hour') return 'Session'
  if (isWeek(w)) return 'Week'
  return w.label.split(' · ')[1] ?? w.label
}

function levelOf(percent: number, pace: number | null): LimitLevel {
  if (pace !== null) return percent >= 100 || percent > pace ? 'over' : percent >= pace * 0.75 ? 'warn' : 'ok'
  return percent >= 90 ? 'over' : percent >= 70 ? 'warn' : 'ok'
}

/**
 * The limits of the login a bot runs on: its agent's reading for the bot's login profile, or the default
 * login's when it has none. The week carries the day and the share a steady pace allows by now. Notes that aren't a
 * share of a cap (today's tokens, a balance) are left out. None while the login has no reading.
 */
export function planLimits(usage: Usage, backend: string, loginProfile: string | null | undefined, now = Date.now()): PlanLimit[] {
  const provider = usage.providers.find((p) => p.id === backend)
  if (!provider) return []
  const windows = loginProfile ? provider.loginProfiles?.find((l) => l.id === loginProfile)?.windows : provider.windows
  return (windows ?? [])
    .filter((w) => !w.text)
    .map((w): PlanLimit => {
      const resetsAt = w.resetsAt ?? null
      const day = isWeek(w) && resetsAt ? Math.min(7, Math.max(1, Math.floor((now - (resetsAt - 7 * DAY)) / DAY) + 1)) : null
      const pace = day === null ? null : Math.round(((day * 100) / 7) * 10) / 10
      return {
        id: w.id,
        title: titleOf(w),
        percent: w.percent,
        reset: resetsAt ? untilShort(resetsAt, now) : null,
        pace,
        day,
        level: levelOf(w.percent, pace),
      }
    })
}
