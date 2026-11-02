// Horizontal cubic beziers for workflow lines. No React.

export interface Point { x: number; y: number }

/** Control-point distance is half the horizontal gap, and at least 48. */
export function edgePath(from: Point, to: Point) {
  const dist = Math.max(Math.abs(to.x - from.x) / 2, 48)
  const c1 = { x: from.x + dist, y: from.y }
  const c2 = { x: to.x - dist, y: to.y }
  return `M ${from.x} ${from.y} C ${c1.x} ${c1.y} ${c2.x} ${c2.y} ${to.x} ${to.y}`
}

export function edgeControls(from: Point, to: Point) {
  const dist = Math.max(Math.abs(to.x - from.x) / 2, 48)
  return { c1: { x: from.x + dist, y: from.y }, c2: { x: to.x - dist, y: to.y } }
}

/** The point halfway along the curve, where a Loop mark or a disconnect sits. */
export function edgeMidpoint(from: Point, to: Point): Point {
  const { c1, c2 } = edgeControls(from, to)
  return {
    x: 0.125 * from.x + 0.375 * c1.x + 0.375 * c2.x + 0.125 * to.x,
    y: 0.125 * from.y + 0.375 * c1.y + 0.375 * c2.y + 0.125 * to.y,
  }
}

function pointAt(from: Point, to: Point, t: number): Point {
  const { c1, c2 } = edgeControls(from, to)
  const u = 1 - t
  return {
    x: u ** 3 * from.x + 3 * u ** 2 * t * c1.x + 3 * u * t ** 2 * c2.x + t ** 3 * to.x,
    y: u ** 3 * from.y + 3 * u ** 2 * t * c1.y + 3 * u * t ** 2 * c2.y + t ** 3 * to.y,
  }
}

/** True when `at` is within `radius` of the curve. */
export function edgeHits(from: Point, to: Point, at: Point, radius = 8) {
  let best = Infinity
  for (let i = 0; i <= 24; i++) {
    const point = pointAt(from, to, i / 24)
    const distance = Math.hypot(point.x - at.x, point.y - at.y)
    if (distance < best) best = distance
  }
  return best <= radius
}
