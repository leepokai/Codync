import type { FindResult } from '../../shared/ipc'

/** The find bar's count: `2/7`, or nothing until something was searched and `No results` when it matched nothing. */
export function findLabel(query: string, result: FindResult | null, noResults: string): string {
  if (!query.trim() || !result) return ''
  return result.matches === 0 ? noResults : `${result.active}/${result.matches}`
}
