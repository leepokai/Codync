/**
 * A reader's place in the rows, kept while a page of earlier ones is asked for.
 *
 * Two things were wrong with the place kept here, and both showed as one symptom: a page of earlier rows arrived
 * and the reader was thrown to parts of the conversation they were not reading.
 *
 * - **It was a distance from the rows' end** (`scrollHeight - scrollTop`). That only holds while nothing below the
 *   reader changes height, and plenty does: the message box grows when a large text is pasted, a picture loads, the
 *   working line comes and goes. It also says nothing about *which* row the reader was on, so it was put back as an
 *   absolute `scrollTop` that had nothing to do with the row they were reading — and once the rows were shorter than
 *   that distance the browser clamped the view to 0, where every later render clamped it again.
 * - **It was applied, and cleared, by the very next render, whichever render that was.** A live chat renders while
 *   the earlier page is still on its way — a message arriving, the working line changing, the message box growing —
 *   so the place was spent before the rows it was kept for arrived. Those rows then went in with nothing left to
 *   compensate for them, and pushed the reader up by their height.
 *
 * So the place is the row at the top edge of the view, it belongs to the one render that shows the rows from an
 * earlier first row (the render the page arrives in), and it is applied as a delta: the view moves by however far
 * that row moved. A row that cannot be found, or a page that brings nothing, leaves the view alone.
 */

/** The row straddling the view's top edge, and how far below that edge its start is (negative: above it). */
export type Place = { id: string; offset: number }

/** A place, and the first row of the rows the reader was reading it from. */
export type KeptPlace = { place: Place; start: string | null }

/**
 * The row at the top edge of the view, from the rows in the order they are shown, each `top` measured from the
 * view's own top. Nothing can start above the first row, so a view scrolled into the rows' top margin keeps that
 * first row and its distance below the top: the reader stays in the margin rather than being pulled to its edge.
 */
export function placeOf(rows: { id: string; top: number }[]): Place | null {
  let found: Place | null = null
  for (const row of rows) {
    if (row.top >= 0) break
    found = { id: row.id, offset: Math.round(row.top) }
  }
  if (found) return found
  const first = rows[0]
  return first ? { id: first.id, offset: Math.round(first.top) } : null
}

/**
 * The place to keep for a page asked for while the rows start at `start`, or null when there is none: a reader at
 * the newest keeps none, because there the rows are brought to the bottom after every render and a place would only
 * fight that.
 */
export function keptPlace(rows: { id: string; top: number }[], start: string | null, following: boolean): KeptPlace | null {
  if (following) return null
  const place = placeOf(rows)
  return place ? { place, start } : null
}

/**
 * How far the view has to move for the kept row to sit where the reader had it: rows that arrived above it pushed it
 * down, so the view moves down by as much. Zero when nothing around it moved, which leaves the view alone.
 */
export function shiftTo(place: Place, rowTop: number) {
  return Math.round(rowTop) - place.offset
}

/**
 * Whether a kept place belongs to this render. Only a render that shows the rows from a different first row can have
 * put any above the reader, so every other render — a message arriving, a picture loading, the message box growing —
 * leaves the view exactly where the reader put it instead of spending the place before the page it was kept for
 * arrives.
 */
export function placeApplies(kept: KeptPlace | null, start: string | null, following: boolean): boolean {
  return !!kept && !following && kept.start !== start
}

/**
 * Whether the rows can scroll at all. Fewer than a screenful, the view sits at its top permanently, so "the reader
 * is near the top" says nothing and earlier rows are not asked for.
 */
export function rowsCanScroll(scrollHeight: number, clientHeight: number) {
  return scrollHeight - clientHeight > 0
}
