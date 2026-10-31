// Camera for the workflow canvas. World = (client - camera) / scale. No React.

export interface Camera { x: number; y: number; scale: number }
export interface Size { width: number; height: number }
export interface Point { x: number; y: number }

export const MIN_SCALE = 0.25
export const MAX_SCALE = 2
export const ZOOM_STEP = 1.2
export const SNAP = 8
export const FIT_MARGIN = 64

export const clampScale = (scale: number) => Math.min(MAX_SCALE, Math.max(MIN_SCALE, scale))

export const snap = (value: number, grid = SNAP) => Math.round(value / grid) * grid

export function worldPoint(camera: Camera, client: Point, origin: Point): Point {
  return { x: (client.x - origin.x - camera.x) / camera.scale, y: (client.y - origin.y - camera.y) / camera.scale }
}

/** Zoom around a viewport point so the world point under it stays put. */
export function zoomAt(camera: Camera, anchor: Point, factor: number): Camera {
  const scale = clampScale(camera.scale * factor)
  const worldX = (anchor.x - camera.x) / camera.scale
  const worldY = (anchor.y - camera.y) / camera.scale
  return { scale, x: anchor.x - worldX * scale, y: anchor.y - worldY * scale }
}

export function zoomStep(camera: Camera, anchor: Point, direction: 1 | -1) {
  return zoomAt(camera, anchor, direction > 0 ? ZOOM_STEP : 1 / ZOOM_STEP)
}

/**
 * Pinch uses 0.007. Ctrl or ⌘ with a mouse wheel uses 0.002.
 * A trackpad pinch arrives as a pixel delta with ctrl held and no ⌘.
 */
export function wheelZoomRatio(input: { deltaY: number; deltaMode: number; ctrlKey: boolean; metaKey: boolean }) {
  if (!input.ctrlKey && !input.metaKey) return null
  const pinch = input.ctrlKey && !input.metaKey && input.deltaMode === 0 && Math.abs(input.deltaY) < 50
  return pinch ? 0.007 : 0.002
}

export function wheelCamera(camera: Camera, input: { deltaX: number; deltaY: number; deltaMode: number; ctrlKey: boolean; metaKey: boolean }, anchor: Point): Camera {
  const ratio = wheelZoomRatio(input)
  if (ratio === null) return { ...camera, x: camera.x - input.deltaX, y: camera.y - input.deltaY }
  return zoomAt(camera, anchor, Math.exp(-input.deltaY * ratio))
}

export interface Bounds { x: number; y: number; width: number; height: number }

/** Scale and translation that frame every node, or the viewport center when the canvas is empty. */
export function fitCamera(nodes: Bounds[], viewport: Size, margin = FIT_MARGIN): Camera {
  if (!nodes.length) return { x: viewport.width / 2, y: viewport.height / 2, scale: 1 }
  const minX = Math.min(...nodes.map((node) => node.x))
  const minY = Math.min(...nodes.map((node) => node.y))
  const maxX = Math.max(...nodes.map((node) => node.x + node.width))
  const maxY = Math.max(...nodes.map((node) => node.y + node.height))
  const contentW = Math.max(maxX - minX, 1)
  const contentH = Math.max(maxY - minY, 1)
  const availableW = Math.max(viewport.width - margin * 2, 1)
  const availableH = Math.max(viewport.height - margin * 2, 1)
  const scale = clampScale(Math.min(availableW / contentW, availableH / contentH))
  return {
    scale,
    x: (viewport.width - contentW * scale) / 2 - minX * scale,
    y: (viewport.height - contentH * scale) / 2 - minY * scale,
  }
}

/** 200ms, or 240ms when the camera travels a long way. */
export function fitDuration(from: Camera, to: Camera) {
  const span = Math.hypot(to.x - from.x, to.y - from.y) + Math.abs(to.scale - from.scale) * 400
  return span > 800 ? 240 : 200
}

export const easeOut = (t: number) => 1 - (1 - t) ** 3

export function lerpCamera(from: Camera, to: Camera, t: number): Camera {
  const k = easeOut(Math.min(1, Math.max(0, t)))
  return { x: from.x + (to.x - from.x) * k, y: from.y + (to.y - from.y) * k, scale: from.scale + (to.scale - from.scale) * k }
}

/** Where a world point sits inside the minimap rectangle. */
export function minimapPoint(world: Point, bounds: Bounds, size: Size): Point {
  const width = Math.max(bounds.width, 1)
  const height = Math.max(bounds.height, 1)
  return { x: ((world.x - bounds.x) / width) * size.width, y: ((world.y - bounds.y) / height) * size.height }
}

export function boundsOf(rects: Bounds[], pad = 80): Bounds {
  if (!rects.length) return { x: -pad, y: -pad, width: pad * 2, height: pad * 2 }
  const minX = Math.min(...rects.map((rect) => rect.x)) - pad
  const minY = Math.min(...rects.map((rect) => rect.y)) - pad
  const maxX = Math.max(...rects.map((rect) => rect.x + rect.width)) + pad
  const maxY = Math.max(...rects.map((rect) => rect.y + rect.height)) + pad
  return { x: minX, y: minY, width: maxX - minX, height: maxY - minY }
}

/** The world rectangle currently inside the viewport. */
export function viewBounds(camera: Camera, viewport: Size): Bounds {
  return { x: -camera.x / camera.scale, y: -camera.y / camera.scale, width: viewport.width / camera.scale, height: viewport.height / camera.scale }
}
