// Where the remote display sits inside the viewer: whole display, letterboxed, no zoom (the
// phones' `ScreenViewport` at zoom 1), inside the viewer's padding. Pure geometry in CSS points;
// the device pixel ratio only enters when asking the computer for a video size.

export interface Point { x: number; y: number }
export interface Size { width: number; height: number }
export interface Rect { x: number; y: number; width: number; height: number }
/** Space kept clear around the video (the rounded frame, the mode switch below it). */
export interface Insets { top: number; right: number; bottom: number; left: number }

export const NO_INSETS: Insets = { top: 0, right: 0, bottom: 0, left: 0 }

export class ScreenViewport {
  /** `display` is the remote display in points (the coordinate space of input), `view` the viewer box in CSS points. */
  readonly display: Size
  readonly view: Size
  readonly insets: Insets

  constructor(display: Size, view: Size, insets: Insets = NO_INSETS) {
    this.display = display
    this.view = view
    this.insets = insets
  }

  /** What the video may fill: the view without its insets. */
  get area(): Rect {
    const { view, insets } = this
    return { x: insets.left, y: insets.top, width: view.width - insets.left - insets.right, height: view.height - insets.top - insets.bottom }
  }

  /** View points per display point. Before layout the area is empty: any positive scale keeps the math finite. */
  get scale(): number {
    const { display, area } = this
    if (display.width <= 0 || display.height <= 0 || area.width <= 0 || area.height <= 0) return 1
    return Math.min(area.width / display.width, area.height / display.height)
  }

  /** Where the video sits in the view; the rest is bars and insets. */
  get frame(): Rect {
    const { display, area, scale } = this
    const width = display.width * scale
    const height = display.height * scale
    return { x: area.x + (area.width - width) / 2, y: area.y + (area.height - height) / 2, width, height }
  }

  /** Whether a view point is on the video rather than in a bar or an inset. */
  contains(p: Point): boolean {
    if (this.area.width <= 0 || this.area.height <= 0) return false
    const f = this.frame
    return p.x >= f.x && p.y >= f.y && p.x < f.x + f.width && p.y < f.y + f.height
  }

  /** The display point under a view point, or null in a black bar. */
  toDisplay(p: Point): Point | null {
    return this.contains(p) ? this.unclamped(p) : null
  }

  /** The display point nearest a view point: a drag that leaves the video keeps its end on the edge. */
  clampToDisplay(p: Point): Point {
    const d = this.unclamped(p)
    return { x: clamp(d.x, 0, Math.max(0, this.display.width - 1)), y: clamp(d.y, 0, Math.max(0, this.display.height - 1)) }
  }

  /** What the area asks the computer to stream: the box in device pixels. */
  pixels(devicePixelRatio: number): Size {
    const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1
    const { area } = this
    return { width: Math.max(1, Math.round(area.width * ratio)), height: Math.max(1, Math.round(area.height * ratio)) }
  }

  private unclamped(p: Point): Point {
    const f = this.frame
    return { x: (p.x - f.x) / this.scale, y: (p.y - f.y) / this.scale }
  }
}

function clamp(value: number, low: number, high: number) {
  return Math.min(Math.max(value, low), high)
}
