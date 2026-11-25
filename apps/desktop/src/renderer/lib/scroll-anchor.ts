/**
 * Where a page of earlier rows leaves the reader, kept as the distance from the rows' end to the view's top.
 *
 * Only a reader who has left the newest keeps one. At the newest the rows are brought to the bottom after every
 * render anyway, and an anchor taken from there would be applied later, once the view has left the bottom, as a
 * distance the rows have since outgrown — which puts the view at its very top, clamped there by the browser.
 */
export function anchorFor(following: boolean, scrollHeight: number, scrollTop: number): number | null {
  return following ? null : scrollHeight - scrollTop
}

/** Where a kept anchor puts the view once the rows are `scrollHeight` tall. */
export function scrollTopFor(anchor: number, scrollHeight: number) {
  return scrollHeight - anchor
}

/**
 * Whether the rows can scroll at all. Fewer than a screenful, the view sits at its top permanently, so "the reader
 * is near the top" says nothing and earlier rows are not asked for.
 */
export function rowsCanScroll(scrollHeight: number, clientHeight: number) {
  return scrollHeight - clientHeight > 0
}
