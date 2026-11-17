// The thinking orb: a halftone disc on the avatars' kind of dot grid. Dots never move; each state is
// a field that sets how full every dot is, so the light flows across the grid. Line for line with
// kit's ThinkingOrbGeometry.swift and Android's ThinkingOrbGeometry.kt; all three are checked
// against apps/ios/Kit/Tests/CodyncKitTests/Fixtures/thinking-orb-frames.json.

export type OrbState = 'working' | 'searching' | 'listening' | 'connecting' | 'shaping'

/** One dot: its cell, its center on the exact grid, its radius and ink opacity. */
export interface OrbDot { col: number; row: number; x: number; y: number; radius: number; alpha: number }

/** One step coarser than the avatars' grid: a status glyph sits beside text. */
export const orbCells = (size: number) => (size < 18 ? 5 : size < 30 ? 7 : 9)

/** The pose a still orb shows (Reduce Motion) and a moving one starts from, in seconds into its animation. */
export const ORB_REST: Record<OrbState, number> = { working: 0.45, searching: 0.45, listening: 0.55, connecting: 0.5, shaping: 1.9 }

const TAU = Math.PI * 2
const clamp01 = (v: number) => Math.max(0, Math.min(1, v))
const smooth = (x: number) => { const c = clamp01(x); return c * c * (3 - 2 * c) }
const lerp = (a: number, b: number, m: number) => a + (b - a) * m
const gauss = (d: number, s: number) => Math.exp(-(d * d) / (2 * s * s))
const fract = (x: number) => x - Math.floor(x)
/** Eased 0 → 1 → 0 over one unit of `x`. */
const pingPong = (x: number) => { const f = fract(x); return f < 0.5 ? smooth(f * 2) : smooth(2 - f * 2) }

// Signed distances on the unit disc (negative inside, y down).
const sdCircle = (u: number, w: number) => Math.hypot(u, w) - 0.62
function sdBox(u: number, w: number) {
  const dx = Math.abs(u) - 0.5, dy = Math.abs(w) - 0.5
  return Math.hypot(Math.max(dx, 0), Math.max(dy, 0)) + Math.min(Math.max(dx, dy), 0)
}
/** An equilateral triangle pointing up, centered on its centroid. */
function sdTriangle(u: number, w: number) {
  const r = 0.62, k = Math.sqrt(3)
  let x = Math.abs(u) - r, y = -(w - 0.04) + r / k
  if (x + k * y > 0) { const nx = (x - k * y) / 2; y = (-k * x - y) / 2; x = nx }
  x -= Math.max(-2 * r, Math.min(0, x))
  return -Math.hypot(x, y) * Math.sign(y)
}
const OUTLINES = [sdCircle, sdTriangle, sdBox]

/** How full the dot at (`u`, `w`) on the unit disc is (0…1), `t` seconds in. */
function fill(state: OrbState, u: number, w: number, t: number) {
  switch (state) {
    case 'working': {
      // A halftone-shaded ball whose light circles it clockwise every 2 s.
      const a = (t * TAU) / 2 - (3 * Math.PI) / 4
      const nz = Math.sqrt(Math.max(0, 1 - u * u - w * w))
      return smooth((0.85 * (u * Math.cos(a) + w * Math.sin(a)) + 0.53 * nz - 0.3) / 0.62)
    }
    case 'searching': {
      // A vertical beam sweeping left and right.
      const x = (pingPong(t / 2.4) * 2 - 1) * 0.85
      return gauss(u - x, 0.25)
    }
    case 'listening': {
      // Rings opening from the center, two at a time.
      const rho = Math.hypot(u, w)
      let v = 0
      for (let k = 0; k < 2; k++) {
        const phase = fract(t / 1.9 + k * 0.5)
        v = Math.max(v, gauss(rho - phase * 1.1, 0.22) * Math.pow(1 - phase, 0.45))
      }
      return v
    }
    case 'connecting': {
      // Two corners lit, a pulse running between them.
      const m = pingPong(t / 1.8), ax = -0.48, ay = 0.48, bx = 0.48, by = -0.48
      const px = lerp(ax, bx, m), py = lerp(ay, by, m)
      return Math.max(0.75 * gauss(Math.hypot(u - ax, w - ay), 0.24), 0.75 * gauss(Math.hypot(u - bx, w - by), 0.24),
        gauss(Math.hypot(u - px, w - py), 0.26))
    }
    case 'shaping': {
      // A filled circle, triangle and square in turn, melting into each other.
      const hold = 1.1, change = 0.6, segment = hold + change
      const cycle = fract(t / (segment * 3)) * segment * 3
      const k = Math.min(2, Math.floor(cycle / segment)), local = cycle - k * segment
      const m = local > hold ? smooth((local - hold) / change) : 0
      const d = lerp(OUTLINES[k]!(u, w), OUTLINES[(k + 1) % 3]!(u, w), m)
      return 1 - smooth((d + 0.1) / 0.22)
    }
  }
}

/** Every dot of the orb in a `size` box at `time` seconds: the grid cropped to a circle. */
export function orbFrame(state: OrbState, size: number, time: number): OrbDot[] {
  if (!(size > 0) || !Number.isFinite(size) || !Number.isFinite(time)) return []
  const n = orbCells(size), pitch = size / n, dots: OrbDot[] = []
  for (let row = 0; row < n; row++) {
    for (let col = 0; col < n; col++) {
      const u = (col + 0.5 - n / 2) / (n / 2), w = (row + 0.5 - n / 2) / (n / 2)
      if (Math.hypot(u, w) > 1.0001) continue
      const v = clamp01(fill(state, u, w, time))
      // A full dot nearly touches its neighbors; an empty one stays a faint grain so the disc keeps its outline.
      dots.push({ col, row, x: (col + 0.5) * pitch, y: (row + 0.5) * pitch, radius: pitch * (0.14 + 0.36 * v), alpha: 0.5 + 0.5 * v })
    }
  }
  return dots
}
