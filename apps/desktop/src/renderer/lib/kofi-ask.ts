/**
 * Active days (days this computer sent a prompt on) at which the Ko-fi invitation comes: three times at most, at the
 * next opening of the chat window once the count is reached. The terminal UI keeps the same days
 * (`host/src/tui/app/kofi.rs`). Desktop and terminal only: the stores' rules on external tip links keep it off the phones.
 */
export const KOFI_ASK_DAYS = [3, 30, 90] as const

/** Days this computer sent a prompt on, and the last one (a local date). */
export type ActiveDays = { count: number; last: string }

/** How many invitations came (`KOFI_ASK_DAYS.length` once Ko-fi was opened), and the active-day count at the last one. */
export type KofiAsk = { asked: number; day: number }

/** `today` counts once. */
export function countDay(days: ActiveDays, today: string): ActiveDays {
  return days.last === today ? days : { count: days.count + 1, last: today }
}

/**
 * The invitation due (1 for the first), or null. Each one waits the days between its mark and the one before, counted
 * from the last, so one that came late never brings the next one sooner.
 */
export function dueKofiAsk(ask: KofiAsk, days: number): number | null {
  const at = KOFI_ASK_DAYS[ask.asked]
  if (at === undefined) return null
  const wait = at - (KOFI_ASK_DAYS[ask.asked - 1] ?? 0)
  return days - ask.day >= wait ? ask.asked + 1 : null
}

/**
 * The invitation an opening of the chat window brings: none while What's new is up (`held`) or a GitHub star ask is
 * due, so one invitation comes at a time.
 */
export function kofiAskAtOpening(ask: KofiAsk, days: number, { held, starDue }: { held: boolean; starDue: boolean }) {
  return held || starDue ? null : dueKofiAsk(ask, days)
}
