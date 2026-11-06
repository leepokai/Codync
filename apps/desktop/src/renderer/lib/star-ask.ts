/** How long after the first ask the second one waits, for people who kept using Codync. */
export const SECOND_ASK_AFTER = 7 * 24 * 60 * 60 * 1000

/** Where the GitHub star ask stands on this computer. */
export type StarAsk = { kind: 'never' } | { kind: 'asked'; at: number } | { kind: 'done' }

/** Which ask is due now: the first one during setup (or after setup for existing installations), the second a week of use later. */
export function dueStarAsk(state: StarAsk, now: number): 'first' | 'second' | null {
  switch (state.kind) {
    case 'never':
      return 'first'
    case 'asked':
      return now - state.at >= SECOND_ASK_AFTER ? 'second' : null
    case 'done':
      return null
  }
}

/** Reopening setup preserves the reminder cooldown; choosing Star always ends asks. */
export function afterStarAsk(state: StarAsk, starred: boolean, now: number): StarAsk {
  if (starred) return { kind: 'done' }
  if (!dueStarAsk(state, now)) return state
  return state.kind === 'never' ? { kind: 'asked', at: now } : { kind: 'done' }
}
