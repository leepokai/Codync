/** A full-shade dot's radius, as a share of the pitch. */
export const DOT_RADIUS = 0.42

/**
 * Where a character's dots go in a `size` box drawn at `scale` device pixels per unit: `cells` per
 * side, `pitch` apart, the first centered at `first`. Dots need whole device pixels: 13 cells over
 * 30 px of a 1x screen alias into a blur, so 13 cells need 4 px each and 9 need 3. Below 2x the pitch
 * is whole pixels too (rounded, or rounded down when that overflows the box by more than a pixel),
 * so every gap is equal. A dot is about a pixel narrower than the pitch: an even width (odd pitch)
 * stays crisp centered on a pixel corner, an odd width on a pixel center. Retina keeps the exact grid.
 */
export function dotGrid(size: number, scale: number) {
  // The canvas is whole device pixels: rounding drops the float error of size × scale.
  const px = Math.round(size * scale)
  const cells = Math.min(size < 18 ? 7 : size < 28 ? 9 : 13, px < 27 ? 7 : px < 52 ? 9 : 13)
  const step = size / cells
  if (scale >= 2) return { cells, pitch: step, first: step / 2 }
  const exact = step * scale
  const rounded = Math.round(exact)
  const devicePitch = Math.max(1, rounded * (cells - 1 + 2 * DOT_RADIUS) <= px + 1 ? rounded : Math.floor(exact))
  const centered = (px - devicePitch * (cells - 1)) / 2
  const first = devicePitch % 2 ? Math.round(centered) : Math.floor(centered) + 0.5
  return { cells, pitch: devicePitch / scale, first: first / scale }
}
