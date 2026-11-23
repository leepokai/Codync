import type { ReactNode } from 'react'

/** One row of the search palette: what it shows, the words a query matches, and what picking it does. */
export interface PaletteItem {
  key: string
  title: string
  subtitle?: string
  leading: ReactNode
  /** Matched by a query, case-insensitively; the title among them. */
  words: string[]
  pick: () => void
}

/** The items matching the query in one of their words; all of them for an empty query. */
export function matchItems(items: PaletteItem[], query: string): PaletteItem[] {
  return matchWords(items, query)
}

/** `matchItems`'s rule over any list that carries words, so a view's own rows match the same way. */
export function matchWords<T extends { words: string[] }>(items: T[], query: string): T[] {
  const q = query.trim().toLowerCase()
  if (!q) return items
  return items.filter((item) => item.words.some((s) => s.toLowerCase().includes(q)))
}
