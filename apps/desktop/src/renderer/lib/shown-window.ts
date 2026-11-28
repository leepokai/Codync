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

/**
 * A row as this arithmetic sees it: separators show no entry; everything else shows the host entries it is about. A
 * grouped row keeps its primary `entry` too, but the other entries in `entries` (approvals) or `exchanges` (bot
 * messages) are what a place can be filed under after a fetch merges older rows into the group.
 */
export type WindowRow = {
  /** The built row's own id, which a separator has and no host entry does. */
  id?: string
  kind: string
  entry?: { id: string }
  entries?: { id: string }[]
  exchanges?: { id: string }[]
}

/** Every host entry a row shows, primary first. */
export function entryIdsOf(row: WindowRow): string[] {
  if (row.entries) return row.entries.map((e) => e.id)
  if (row.exchanges) return row.exchanges.map((e) => e.id)
  return row.entry ? [row.entry.id] : []
}

/**
 * The ids that identify the first shown row and every row after it: the row's own id (a separator has one too) and
 * every host entry it shows. A window pinned to an index alone is lost when a row above it goes away (a message taken
 * back) or several rows collapse into one (settled approvals grouping): the same index then names a row the reader
 * never saw. These give the pin something stable to follow.
 */
export function rowAnchors(rows: WindowRow[], at: number): string[] {
  const seen = new Set<string>()
  const out: string[] = []
  for (const row of rows.slice(Math.max(0, at))) {
    for (const id of [...(row.id ? [row.id] : []), ...entryIdsOf(row)]) {
      if (seen.has(id)) continue
      seen.add(id)
      out.push(id)
    }
  }
  return out
}

/**
 * Anchors for a window. The newest page has none: it follows the end, so pinning it to the entries it happened to
 * show would stop new arrivals from moving the page's start and leave the follower with one row too many.
 */
export function windowAnchors(rows: WindowRow[], from: WindowStart, page: number): string[] {
  return from === null ? [] : rowAnchors(rows, windowStart(rows.length, from, page))
}

/** The first row named by any of `anchors`, in the list's order, or null when every one of them is gone. */
export function indexOfAnyAnchor(rows: WindowRow[], anchors: string[]): number | null {
  if (anchors.length === 0) return null
  const wanted = new Set(anchors)
  const at = rows.findIndex((row) => (row.id !== undefined && wanted.has(row.id)) || entryIdsOf(row).some((id) => wanted.has(id)))
  return at < 0 ? null : at
}

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
 * Where a window starts after rows above it were removed or regrouped. Its row anchors win over its old index: the
 * first anchor still in the list is the first row the reader was seeing, wherever the list has since put it. Only a
 * window with no anchors left (or a plain numeric one from an older caller) falls back to the clamped index.
 */
export function pinnedWindowStart(rows: WindowRow[], from: WindowStart, anchors: string[], page: number): number {
  return indexOfAnyAnchor(rows, anchors) ?? windowStart(rows.length, from, page)
}

/**
 * Where the entry the reader's first row shows now sits in a freshly rebuilt list, or null when it is not there:
 * separators (which show none) are skipped — they are decoration the grouping adds, not a place. A grouped row is
 * found by any of the host entries inside it, because fetching older rows can merge the entry the place was filed
 * under into a group whose primary `entry` is an older one.
 */
export function indexOfEntry(rows: WindowRow[], entry: string | null): number | null {
  if (!entry) return null
  const at = rows.findIndex((row) => row.kind !== 'separator' && entryIdsOf(row).includes(entry))
  return at < 0 ? null : at
}

/**
 * The entry shown by the first row at or after `at`: the reader's place in the conversation, as opposed to the
 * separator that happens to sit above it in this list. Null for a list with no entries in it.
 */
export function entryFrom(rows: WindowRow[], at: number): string | null {
  for (let i = Math.max(0, at); i < rows.length; i++) {
    const row = rows[i]!
    if (row.kind !== 'separator') return entryIdsOf(row)[0] ?? null
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
 * What one "load earlier" press does to a list already held. `start` is the page above the reader; `fetch` says the
 * press also has to ask the computer. That is true at the start of the loaded list, including when the only cached
 * row above is a date separator: exposing that separator and stopping would make older messages take a second press.
 */
export function pagePlan(rows: WindowRow[], wasShowing: string | null, page: number, canLoadOlder: boolean): {
  start: number | null
  fetch: boolean
} {
  const above = indexOfEntry(rows, wasShowing)
  // A catch-up can be all trace: the computer still has older chat rows, but `buildChat` has filtered every held
  // entry out. There is no entry to page above, so ask the host rather than treating an empty visible list as the end.
  // One press makes one request; if it returns only trace too, the next press may ask again.
  if (above === null) return { start: null, fetch: canLoadOlder && !rows.some(showsEntry) }
  const start = pageAbove(above, page)
  // At index 0 the page above is whatever is already held. If that contains a visible entry, it is a cached page and
  // opens without the computer even when the host has more. Only when it holds no visible row (nothing above, or a
  // lone separator) does the press have to fetch, or it would stop one separator short of older messages.
  const cachedPage = rows.slice(0, above).some(showsEntry)
  return { start, fetch: start === 0 && canLoadOlder && !cachedPage }
}

/** Whether a row shows a host entry rather than being decoration only. */
function showsEntry(row: WindowRow): boolean {
  return row.kind !== 'separator' && entryIdsOf(row).length > 0
}

/**
 * The window a reveal asks for. A follower whose card is already in the newest page stays a follower: installing a
 * numeric window there leaves anchors behind that stop the next arrival from moving the page, so the transcript grows
 * past one page. A card further back opens the history at it.
 */
export function revealWindowStart(rows: WindowRow[], entry: string | null, from: WindowStart, page: number, following: boolean): WindowStart {
  const at = indexOfEntry(rows, entry)
  // A card that is not in the list leaves the window alone. A follower has no explicit window to leave, so it stays
  // the plain newest page rather than acquiring anchors from a stale numeric start.
  if (at === null) return following ? null : from
  const newest = windowStart(rows.length, null, page)
  // A card inside the newest page never needs history, even when the follower previously opened a cached page. Keeping
  // the expanded numeric start here would leave 80 rows shown and grow to 81 when the next row arrives.
  if (following && at >= newest) return null
  const start = Math.min(at, windowStart(rows.length, from, page))
  return following && start >= newest ? null : start
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
