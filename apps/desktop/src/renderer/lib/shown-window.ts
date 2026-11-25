/**
 * The window `ChatLog` keeps into a conversation longer than one page: where it starts, and where a page of earlier
 * rows leaves it.
 *
 * Two things about this went wrong, and together they are the reported bug: paging up through the history threw the
 * reader to parts of the conversation they were not reading, and PageDown could not bring them back. Both were
 * measured on a real host before this was rewritten.
 *
 * - **The window was filed under the id of the row it started at**, and a row's id is not a position. `buildChat`
 *   always puts a date separator above the first row it builds, with the id `sep-<the entry below it>`, so the first
 *   row of a loaded list is normally a separator — a row that exists *only because the list began there*. One page of
 *   older entries later it is gone, because the gap it marked is now filled, so looking that id up finds nothing.
 *   `showEarlier` then read "nothing above this" (`Math.max(0, findIndex(...))` → `0`) and asked for no page, while
 *   the window's own key could not be found either and fell back to the newest page. The reader, who had just paged
 *   to the oldest rows held, was put back on the newest ones — 261 rows away in the measured case — and every later
 *   press walked into the same trap.
 * - **The next page was found by looking an id up in the list the fetch had just rebuilt.** Merges move; a row that
 *   merged into its neighbour keeps the neighbour's id. The index was then arithmetic in a coordinate space the
 *   reader had never been in.
 *
 * So the window is a **number**: where in the loaded list it starts. A number cannot go missing. What does have to be
 * tracked across a fetch is the **entry** the reader's first row shows — a host id that is stable however the rows
 * are grouped — and, from it, where in the rebuilt list that entry now sits. The page goes in one page above it.
 */

/** Where the shown rows start: an index into the loaded rows, or null for the newest `page` rows. */
export type WindowStart = number | null

/** A row as this arithmetic sees it: separators show no entry; everything else shows the host entry it is about. */
export type WindowRow = { kind: string; entry?: { id: string } }

/**
 * The row the shown rows start at, given how many rows the list holds and where the window was. A window is clamped
 * into the list, so entries going away (a message taken back, a chat compacted) keeps as much of what is left as it
 * can, instead of jumping to the newest page.
 */
export function windowStart(total: number, from: WindowStart, page: number): number {
  if (from === null) return Math.max(0, total - page)
  return Math.min(Math.max(0, from), Math.max(0, total - 1))
}

/**
 * Where the entry the reader's first row shows now sits in a freshly rebuilt list, or null when it is not there:
 * `entry?.id` is the host entry, and separators (which show none) are skipped — they are decoration the grouping
 * adds, not a place.
 */
export function indexOfEntry(rows: WindowRow[], entry: string | null): number | null {
  if (!entry) return null
  const at = rows.findIndex((row) => row.kind !== 'separator' && row.entry?.id === entry)
  return at < 0 ? null : at
}

/**
 * The entry shown by the first row at or after `at`: the reader's place in the conversation, as opposed to the
 * separator that happens to sit above it in this list. Null for a list with no entries in it.
 */
export function entryFrom(rows: WindowRow[], at: number): string | null {
  for (let i = Math.max(0, at); i < rows.length; i++) {
    const row = rows[i]!
    if (row.kind !== 'separator' && row.entry) return row.entry.id
  }
  return null
}

/**
 * The window start that shows a page of earlier rows above `above`, the row the reader's first entry now sits at:
 * the page goes in directly above the rows they are reading, not at the fetched page's own edge.
 */
export function pageAbove(above: number, page: number): number {
  return Math.max(0, above - page)
}

/**
 * The window start that shows `entry` and keeps at least the rows already shown: a task's card further down brings
 * the window down to it, and a card already shown leaves it alone.
 */
export function startShowing(rows: WindowRow[], entry: string | null, from: WindowStart, page: number): WindowStart {
  const at = indexOfEntry(rows, entry)
  if (at === null) return from
  return Math.min(at, windowStart(rows.length, from, page))
}
