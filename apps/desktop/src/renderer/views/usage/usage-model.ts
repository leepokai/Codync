import type { UsageProvider, UsageWindow } from '@shared/models'
import { isDark } from '../../lib/theme'

/** Claude keeps its orange; everyone else is ink, like the rest of the app. */
export function providerTint(id: string) {
  return id === 'claude' ? '#D97757' : isDark() ? '#FFFFFF' : '#000000'
}

/** CSS color for views (follows the scheme by itself). */
export const providerTintCSS = (id: string) => (id === 'claude' ? '#D97757' : 'var(--accent)')

/** ACP registry id of the provider's agent, for its logo. */
export const providerRegistry = (id: string) => (['claude', 'codex'].includes(id) ? `${id}-acp` : id)

/** The character that stands for this provider. */
export const mascotShape = (id: string) => (id === 'codex' ? 'hex' : 'blob')

/** A plan window. Notes (today's tokens, a dollar balance) are not limits. */
export const isLimit = (w: UsageWindow) => !w.text

export const tightest = (p: UsageProvider) =>
  p.windows.filter(isLimit).reduce<UsageWindow | null>((a, w) => (!a || w.percent > a.percent ? w : a), null)

/** "62% used": the share of the cap already spent, said so (a bare percent reads as what's left). */
export const usedText = (percent: number, tr: (text: string) => string) =>
  tr('{percent}% used').replace('{percent}', String(Math.round(percent)))

/** The used percent, or the note when this window isn't a share of a cap. */
export function windowReading(w: UsageWindow, tr: (text: string) => string) {
  return w.text || usedText(w.percent, tr)
}

/** "Session" for the 5-hour window, "Opus" for "Weekly · Opus", else the label. */
export function windowTitle(w: UsageWindow) {
  if (w.label === '5-hour') return 'Session'
  const model = w.label.split(' · ')[1]
  return model ?? w.label
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/**
 * Claude Code's text: "Sep 26 at 12pm (Asia/Taipei)", "Sep 25 at 7:30pm (Asia/Taipei)", or just
 * "7:30pm (…)". The year isn't given: the next such date from `now`.
 */
export function parseReset(text: string, now = new Date()): Date | null {
  const m = /^(?:(\w{3}) (\d{1,2}) at )?(\d{1,2})(?::(\d{2}))?(am|pm)(?: \(([^)]+)\))?$/.exec(text)
  if (!m) return null
  const zone = m[6]
  // Wall time in `zone` → epoch: guess, then correct by the zone's offset at that instant.
  const parts = zonedParts(now, zone)
  let month = parts.month
  let day = parts.day
  if (m[1]) {
    const mi = MONTHS.indexOf(m[1])
    if (mi < 0) return null
    month = mi + 1
    day = Number(m[2])
  }
  const hour = (Number(m[3]) % 12) + (m[5] === 'pm' ? 12 : 0)
  const minute = m[4] ? Number(m[4]) : 0
  const at = (year: number, mo: number, d: number) => fromZoned(year, mo, d, hour, minute, zone)
  let date = at(parts.year, month, day)
  if (date.getTime() < now.getTime() - 86_400_000 || (!m[1] && date < now)) {
    date = m[1] ? at(parts.year + 1, month, day) : new Date(date.getTime() + 86_400_000)
  }
  return date
}

function zonedParts(date: Date, zone?: string) {
  try {
    const f = new Intl.DateTimeFormat('en-US', { timeZone: zone, year: 'numeric', month: 'numeric', day: 'numeric' })
    const p = Object.fromEntries(f.formatToParts(date).map((x) => [x.type, x.value]))
    return { year: Number(p.year), month: Number(p.month), day: Number(p.day) }
  } catch {
    return { year: date.getFullYear(), month: date.getMonth() + 1, day: date.getDate() }
  }
}

function fromZoned(year: number, month: number, day: number, hour: number, minute: number, zone?: string) {
  const guess = Date.UTC(year, month - 1, day, hour, minute)
  try {
    const f = new Intl.DateTimeFormat('en-US', { timeZone: zone, hourCycle: 'h23', year: 'numeric', month: 'numeric', day: 'numeric', hour: 'numeric', minute: 'numeric' })
    const p = Object.fromEntries(f.formatToParts(new Date(guess)).map((x) => [x.type, x.value]))
    const asZone = Date.UTC(Number(p.year), Number(p.month) - 1, Number(p.day), Number(p.hour), Number(p.minute))
    return new Date(guess - (asZone - guess))
  } catch {
    return new Date(year, month - 1, day, hour, minute)
  }
}

export function resetDate(w: UsageWindow): Date | null {
  if (w.resetsAt) return new Date(w.resetsAt)
  return w.resetsText ? parseReset(w.resetsText) : null
}

/** "3h 20m", "2d", "45m". */
export function untilText(date: Date, now = new Date()) {
  const s = Math.max(0, (date.getTime() - now.getTime()) / 1000)
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  if (h >= 48) return `${Math.floor(h / 24)}d`
  if (h > 0) return `${h}h ${m}m`
  return `${m}m`
}

/** "resets in 3h 20m" / "resets Sep 26 at 12pm", or null. */
export function resetDescription(w: UsageWindow) {
  const d = resetDate(w)
  if (d) return `resets in ${untilText(d)}`
  return w.resetsText ? `resets ${w.resetsText.replace(/ \(.*\)$/, '')}` : null
}
