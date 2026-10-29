import type { PointerEvent as ReactPointerEvent } from 'react'

/** How far the pointer travels before a press becomes a drag. */
const THRESHOLD = 4

export interface Point {
  x: number
  y: number
}

export interface PointerDragHandlers {
  /** The press (at `from`) has moved far enough: the drag starts. */
  start: (from: Point) => void
  /** The pointer moved while dragging: where it is, and what's under it. */
  over: (at: Point, under: Element | null) => void
  /** Released after dragging. */
  drop: () => void
  /** Escape, a cancelled pointer, or the window losing focus. */
  cancel: () => void
}

/**
 * Reorders by dragging with the pointer instead of native drag and drop, whose cursor a page can't
 * set: past a few points the drag starts, every element shows a closed hand, `over` reports what's
 * under the pointer, and the click that ends the drag is swallowed. Spread the result of
 * `pointerDrag(handlers)` on the element that's held.
 */
export function pointerDrag(handlers: PointerDragHandlers) {
  return {
    onPointerDown(event: ReactPointerEvent) {
      if (event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey) return
      const origin = { x: event.clientX, y: event.clientY }
      let dragging = false
      const end = () => {
        window.removeEventListener('pointermove', move)
        window.removeEventListener('pointerup', up)
        window.removeEventListener('pointercancel', cancel)
        window.removeEventListener('keydown', key, true)
        window.removeEventListener('blur', cancel)
        document.documentElement.classList.remove('pointer-dragging')
      }
      const move = (e: PointerEvent) => {
        if (!dragging) {
          if (Math.hypot(e.clientX - origin.x, e.clientY - origin.y) < THRESHOLD) return
          dragging = true
          document.documentElement.classList.add('pointer-dragging')
          handlers.start(origin)
        }
        handlers.over({ x: e.clientX, y: e.clientY }, document.elementFromPoint(e.clientX, e.clientY))
      }
      const up = () => {
        end()
        if (!dragging) return
        handlers.drop()
        swallowNextClick()
      }
      const cancel = () => {
        end()
        if (dragging) handlers.cancel()
      }
      const key = (e: KeyboardEvent) => {
        if (e.key !== 'Escape' || !dragging) return
        e.preventDefault()
        e.stopPropagation()
        cancel()
        // The release still to come is the end of this gesture (unless a new press comes first).
        window.addEventListener('pointerup', swallowNextClick, { once: true })
        window.addEventListener('pointerdown', () => window.removeEventListener('pointerup', swallowNextClick), { once: true })
      }
      window.addEventListener('pointermove', move)
      window.addEventListener('pointerup', up)
      window.addEventListener('pointercancel', cancel)
      window.addEventListener('keydown', key, true)
      window.addEventListener('blur', cancel)
    },
  }
}

/** The click a browser sends after the release that ended a drag must not open what was held. */
function swallowNextClick() {
  const swallow = (e: MouseEvent) => {
    e.preventDefault()
    e.stopPropagation()
  }
  window.addEventListener('click', swallow, { capture: true, once: true })
  setTimeout(() => window.removeEventListener('click', swallow, true), 0)
}
