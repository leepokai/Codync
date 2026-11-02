// Where the remote display sits inside the viewer: whole display, letterboxed, no zoom (the
// phones' `ScreenViewport` at zoom 1). Pure geometry in CSS points; the device pixel ratio only
// enters when asking the computer for a video size.

export interface Point { x: number; y: number }
export interface Size { width: number; height: number }
export interface Rect { x: number; y: number; width: number; height: number }

export class ScreenViewport {
  /** `display` is the remote display in points (the coordinate space of input), `view` the viewer box in CSS points. */
  readonly display: Size
  readonly view: Size

  constructor(display: Size, view: Size) {
    this.display = display
    this.view = view
  }

  /** View points per display point. Before layout the view is 0×0: any positive scale keeps the math finite. */
  get scale(): number {
    const { display, view } = this
    if (display.width <= 0 || display.height <= 0 || view.width <= 0 || view.height <= 0) return 1
    return Math.min(view.width / display.width, view.height / display.height)
  }

  /** Where the video sits in the view; the rest is black bars. */
  get frame(): Rect {
    const { display, view, scale } = this
    const width = display.width * scale
    const height = display.height * scale
    return { x: (view.width - width) / 2, y: (view.height - height) / 2, width, height }
  }

  /** Whether a view point is on the video rather than in a bar. */
  contains(p: Point): boolean {
    if (this.view.width <= 0 || this.view.height <= 0) return false
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

  /** What `view` asks the computer to stream: the box in device pixels (`long` and `short` side). */
  pixels(devicePixelRatio: number): Size {
    const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1
    return { width: Math.max(1, Math.round(this.view.width * ratio)), height: Math.max(1, Math.round(this.view.height * ratio)) }
  }

  private unclamped(p: Point): Point {
    const f = this.frame
    return { x: (p.x - f.x) / this.scale, y: (p.y - f.y) / this.scale }
  }
}

function clamp(value: number, low: number, high: number) {
  return Math.min(Math.max(value, low), high)
}
