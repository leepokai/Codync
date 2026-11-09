/** The prompt sent from this computer that brings the second ask. */
export const SECOND_ASK_AT = 5

/** Where the GitHub star ask stands on this computer. */
export type StarAsk = { kind: 'never' } | { kind: 'asked' } | { kind: 'done' }

/** Which ask is due now: the first one during setup (or after setup for existing installations), the second at the fifth prompt. */
export function dueStarAsk(state: StarAsk, prompts: number): 'first' | 'second' | null {
  switch (state.kind) {
    case 'never':
      return 'first'
    case 'asked':
      return prompts >= SECOND_ASK_AT ? 'second' : null
    case 'done':
      return null
  }
}

/** Reopening setup keeps the second ask waiting; choosing Star always ends asks. */
export function afterStarAsk(state: StarAsk, starred: boolean, prompts: number): StarAsk {
  if (starred) return { kind: 'done' }
  if (!dueStarAsk(state, prompts)) return state
  return state.kind === 'never' ? { kind: 'asked' } : { kind: 'done' }
}
