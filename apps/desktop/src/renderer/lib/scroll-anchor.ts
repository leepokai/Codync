/**
 * A reader's place in the rows, kept while earlier ones are going in above them.
 *
 * The place used to be a distance from the rows' end (`scrollHeight - scrollTop`). That holds only while nothing
 * below the reader changes height, and plenty does — the message box grows with a pasted draft, a picture loads, the
 * working line comes and goes. It also said nothing about *which* row the reader was on, so it was put back as an
 * absolute `scrollTop` unrelated to what they were reading, and once the rows were shorter than that distance the
 * browser clamped the view to 0.
 *
 * The place is now the row straddling the view's top edge.
 *
 * The measurement is taken in the rows' own space, not the view's: `top` is where the row starts in the content, so a
 * scroll of the view never looks like a change to the rows. Only something inserted or removed above the row moves
 * it, and that is exactly what has to be paid for. A reader scrolling their own view therefore needs no guard at all:
 * their scroll changes the view's position, not the row's place in the content.
 */

/** The row straddling the view's top edge, and where it starts in the rows' own space. */
export type Place = { id: string; top: number }

/** A row as the place is measured: its id, its start in the content, and how far below the view's edge that start is. */
export type PlaceRow = { id: string; contentTop: number; viewTop: number }

/**
 * The row at the top edge of the view, from the rows in the order they are shown. Nothing starts above the first
 * row, so a view scrolled into the rows' top margin keeps that first row, with its own start: the reader stays in the
 * margin rather than being pulled to its edge.
 */
export function placeOf(rows: PlaceRow[]): Place | null {
  let found: Place | null = null
  for (const row of rows) {
    if (row.viewTop >= 0) break
    found = { id: row.id, top: Math.round(row.contentTop) }
  }
  if (found) return found
  const first = rows[0]
  return first ? { id: first.id, top: Math.round(first.contentTop) } : null
}

/**
 * The place to keep while a page is asked for, or null for a reader following the newest: there the rows are brought
 * to the bottom after every render, and a place would only fight that.
 */
export function keptPlace(rows: PlaceRow[], following: boolean): Place | null {
  return following ? null : placeOf(rows)
}

/**
 * How far the view has to move for the kept row to sit where the reader had it. Rows that went in above it pushed it
 * down by their height, so the view moves down by as much; nothing above it moved (a message arriving below, the
 * message box growing) leaves it at zero, and the view alone.
 */
export function shiftTo(place: Place, contentTop: number) {
  return Math.round(contentTop) - place.top
}

/**
 * The place as it stands once the view has moved for it: the row's start, read back after the move.
 *
 * A place is a difference, and moving the view does not close a difference — the row's start in the content is
 * the same number however the view is scrolled. So a place left exactly as it was taken reads the same difference
 * on the next render and pays it again, and the view walks a page further every render there is no new row on.
 * Measured on the reported bug: 0 → 2120 → 4240 → 6360 → 8480 → 10600, five renders of a streaming reply.
 *
 * The place has to be the row's start *as it now is*, and that is what makes a second render find nothing left to
 * move. A view clamped short of the distance it was asked for lands somewhere the row does not start, and reading
 * it back is what accepts that: the hold becomes where the row actually is, once, instead of the full distance the
 * reader was owed.
 */
export function settledPlace(place: Place, contentTop: number): Place {
  return { id: place.id, top: Math.round(contentTop) }
}

/**
 * Whether the rows can scroll at all. Fewer than a screenful, the view sits at its top permanently, so "the reader
 * is near the top" says nothing and earlier rows are not asked for.
 */
export function rowsCanScroll(scrollHeight: number, clientHeight: number) {
  return scrollHeight - clientHeight > 0
}

/**
 * Every shown row's place: its id, its start in the content (the view's own scroll added back), and how far below the
 * view's top edge that start is. One read of the DOM per row, in one pass.
 */
export function placesOf(el: HTMLElement): PlaceRow[] {
  const viewTop = el.getBoundingClientRect().top
  const scrolled = el.scrollTop
  return [...el.querySelectorAll<HTMLElement>('[data-entry-id]')].map((row) => {
    const toView = row.getBoundingClientRect().top - viewTop
    return { id: row.dataset.entryId!, viewTop: toView, contentTop: toView + scrolled }
  })
}

/** Where a row starts in the rows' own space: how far below the view's top edge it is, plus how far the view is scrolled. */
export function contentTopOf(el: HTMLElement, row: HTMLElement) {
  return row.getBoundingClientRect().top - el.getBoundingClientRect().top + el.scrollTop
}
