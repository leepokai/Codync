/** A full-shade dot's radius, as a share of the pitch: large dots with a clear gap between them. */
export const DOT_RADIUS = 0.4

/**
 * The radius of the dot centered at (`x`, `y`) in a `size` box, for its shade (0.3…1). Below 2x the
 * snapped grid can put an edge dot less than a radius from the box edge; that dot is cut to fit.
 */
export function dotRadius(pitch: number, shade: number, x: number, y: number, size: number) {
  return Math.min(pitch * DOT_RADIUS * (0.7 + 0.3 * shade), x, size - x, y, size - y)
}

/**
 * Where a character's dots go in a `size` box drawn at `scale` device pixels per unit: `cells` per
 * side, `pitch` apart, the first centered at `first`. Every character is 9 × 9 large dots; tiny ones
 * are 7 × 7. Dots need whole device pixels, so 9 cells need 3 px each (27 px). Below 2x the pitch
 * is whole pixels too (rounded, or rounded down when full dots would overflow the box by more than
 * a pixel), so every gap is equal. A dot is about a pixel narrower than the pitch: an even width
 * (odd pitch) stays crisp centered on a pixel corner, an odd width on a pixel center. Retina keeps
 * the exact grid.
 */
export function dotGrid(size: number, scale: number) {
  // The canvas is whole device pixels: rounding drops the float error of size × scale.
  const px = Math.round(size * scale)
  const cells = size < 18 || px < 27 ? 7 : 9
  const step = size / cells
  if (scale >= 2) return { cells, pitch: step, first: step / 2 }
  const exact = step * scale
  const rounded = Math.round(exact)
  const devicePitch = Math.max(1, rounded * (cells - 1 + 2 * DOT_RADIUS) <= px + 1 ? rounded : Math.floor(exact))
  const centered = (px - devicePitch * (cells - 1)) / 2
  const first = devicePitch % 2 ? Math.round(centered) : Math.floor(centered) + 0.5
  return { cells, pitch: devicePitch / scale, first: first / scale }
}
