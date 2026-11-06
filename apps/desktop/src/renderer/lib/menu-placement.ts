/** Floating menu geometry, in window coordinates. */
interface Size { width: number; height: number }
interface Rect extends Size { left: number; top: number; right: number }
const INSET = 8
const GAP = 4
export const ACTION_MENU_WIDTH = 260

export function menuPlacement(point: { x: number; y: number }, size: Size, view: Size) {
  const width = Math.min(size.width, Math.max(0, view.width - INSET * 2))
  return {
    width,
    left: clamp(point.x, view.width - width - INSET),
    top: clamp(point.y, view.height - size.height - INSET),
  }
}

/** Prefer beside the parent, flip left when needed, and align with the hovered row. */
export function submenuPlacement(parent: Rect, row: { top: number }, size: Size, view: Size) {
  const right = parent.right + GAP
  const left = right + size.width <= view.width - INSET ? right : parent.left - GAP - size.width
  return menuPlacement({ x: left, y: row.top - 7 }, size, view)
}

function clamp(value: number, maximum: number) {
  return Math.max(INSET, Math.min(value, maximum))
}
