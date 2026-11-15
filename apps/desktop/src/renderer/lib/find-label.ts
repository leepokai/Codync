/** Where the find stands: the match shown (1-based, 0 when none) out of all of them. */
export interface FindState {
  active: number
  matches: number
}

/** The find bar's count: `2/7`, nothing until something was searched, and `No results` when it matched nothing. */
export function findLabel(query: string, state: FindState, noResults: string): string {
  if (!query.trim()) return ''
  return state.matches === 0 ? noResults : `${state.active}/${state.matches}`
}
