/**
 * Whether a one-line box is clipping its text: `truncate` (styles/theme.css) hides the overflow and
 * ends the line in an ellipsis, so a `scrollWidth` wider than the `clientWidth` means the reader
 * cannot see the end of it. A row uses this to decide whether a hover tip has anything to add — a
 * path that fits needs none.
 *
 * The comparison is on the box, never on the string: a short path in a narrow sheet clips too. One
 * pixel of slack absorbs a rounded layout, so a box that fits is not called clipped by sub-pixel
 * sizing.
 */
export const isTruncated = (scrollWidth: number, clientWidth: number) => scrollWidth > clientWidth + 1
