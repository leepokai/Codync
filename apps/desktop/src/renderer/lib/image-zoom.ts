/** Pinch-to-zoom for a picture shown in a box: where it sits and how big it is, and the moves that change that. */

export interface Zoom {
  scale: number
  /** How far the picture is moved from the box's center, in the box's pixels. */
  x: number
  y: number
}

export interface Size {
  width: number
  height: number
}

export const FIT: Zoom = { scale: 1, x: 0, y: 0 }
export const MAX_SCALE = 8
/** Double-click zooms to this much. */
export const DOUBLE_CLICK_SCALE = 2.5

const limit = (value: number, low: number, high: number) => Math.min(Math.max(value, low), high)

/** The picture stays inside the box when it is smaller than it, and never leaves a gap at an edge when it is larger. */
export function clampZoom(zoom: Zoom, box: Size, picture: Size): Zoom {
  const scale = limit(zoom.scale, 1, MAX_SCALE)
  const reach = (boxSide: number, pictureSide: number) => Math.max(0, (pictureSide * scale - boxSide) / 2)
  const [rx, ry] = [reach(box.width, picture.width), reach(box.height, picture.height)]
  // `|| 0` turns a -0 into 0.
  return { scale, x: limit(zoom.x, -rx, rx) || 0, y: limit(zoom.y, -ry, ry) || 0 }
}

/** Zooms by `factor` keeping the spot under `point` (from the box's center) where it is. */
export function zoomBy(zoom: Zoom, factor: number, point: { x: number; y: number }, box: Size, picture: Size): Zoom {
  const scale = limit(zoom.scale * factor, 1, MAX_SCALE)
  const ratio = scale / zoom.scale
  return clampZoom({ scale, x: point.x - (point.x - zoom.x) * ratio, y: point.y - (point.y - zoom.y) * ratio }, box, picture)
}

export function panBy(zoom: Zoom, dx: number, dy: number, box: Size, picture: Size): Zoom {
  return clampZoom({ ...zoom, x: zoom.x + dx, y: zoom.y + dy }, box, picture)
}

/** The zoom a double-click goes to: back to fit when zoomed in, else in on the spot clicked. */
export function toggleZoom(zoom: Zoom, point: { x: number; y: number }, box: Size, picture: Size): Zoom {
  return zoom.scale > 1 ? FIT : zoomBy(zoom, DOUBLE_CLICK_SCALE, point, box, picture)
}

/** A trackpad pinch arrives as wheel events with ctrl held: each one's `deltaY` is a small step of the pinch. */
export function pinchFactor(deltaY: number, deltaMode = 0) {
  const pixels = deltaMode === 1 ? deltaY * 16 : deltaY
  return Math.exp(-limit(pixels, -50, 50) * 0.01)
}

export interface Touch {
  x: number
  y: number
}

/** Two fingers moving: the change in their distance is the zoom, and their middle moving is the pan. `origin` is where the middle was. */
export function pinchStep(before: [Touch, Touch], after: [Touch, Touch]) {
  const distance = ([a, b]: [Touch, Touch]) => Math.hypot(a.x - b.x, a.y - b.y)
  const middle = ([a, b]: [Touch, Touch]) => ({ x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 })
  const [from, to] = [distance(before), distance(after)]
  const [m0, m1] = [middle(before), middle(after)]
  return { factor: from > 0 ? to / from : 1, origin: m0, dx: m1.x - m0.x, dy: m1.y - m0.y }
}
