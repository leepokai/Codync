/** Where the find's current match sits as the page changes: which match stays current, and how far to scroll to show it. */

/**
 * The match to stay on after the list of matches was rebuilt (earlier messages loaded, a reply streamed in): the same
 * occurrence if it is still there, else the next one after where it was, else the last. `compare` orders two matches
 * by where they start in the page (negative when the first comes before the second).
 */
export function keptMatch<T>(matches: readonly T[], previous: T, compare: (a: T, b: T) => number): number {
  const at = matches.findIndex((match) => compare(match, previous) >= 0)
  return at >= 0 ? at : Math.max(matches.length - 1, 0)
}

/** A stretch along one axis of the page: where it starts and how long it is. */
export interface Span {
  start: number
  size: number
}

/** How far a box showing `view` scrolls to bring `item` to its middle. */
export function centeringOffset(item: Span, view: Span): number {
  return item.start - view.start - (view.size - item.size) / 2
}

/** How far a box showing `view` scrolls to bring `item` to its middle, or 0 when all of `item` already shows. */
export function offsetToShow(item: Span, view: Span): number {
  const shown = item.start >= view.start && item.start + item.size <= view.start + view.size
  return shown ? 0 : centeringOffset(item, view)
}
