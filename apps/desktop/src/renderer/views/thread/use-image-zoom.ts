import { useCallback, useEffect, useRef, useState, type CSSProperties, type PointerEvent } from 'react'
import { FIT, clampZoom, panBy, pinchFactor, pinchStep, toggleZoom, zoomBy, type Touch, type Zoom } from '../../lib/image-zoom'

const KEY_STEP = 1.25

/**
 * The state and gestures of a zoomable picture: pinch on the trackpad (or ctrl + wheel), two fingers, double-click,
 * `+` / `-` / `0`, and dragging or scrolling to move it once zoomed in. The box it measures holds the picture as its first child.
 */
export function useImageZoom(onZoom?: () => void) {
  const box = useRef<HTMLDivElement>(null)
  const [zoom, setZoom] = useState<Zoom>(FIT)
  const current = useRef(zoom)
  const touches = useRef(new Map<number, Touch>())
  const zoomed = useRef(onZoom)
  zoomed.current = onZoom

  const apply = useCallback((next: Zoom, changesScale = false) => {
    current.current = next
    setZoom(next)
    // Counted when the picture is actually zoomed in, not when a zoom out or reset leaves it at fit.
    if (changesScale && next.scale > 1) zoomed.current?.()
  }, [])

  /** The box, the picture's fit size (its layout size: the zoom is a transform), and a screen point from the box's center. */
  const measure = useCallback(() => {
    const el = box.current!
    const picture = el.firstElementChild as HTMLElement | null
    const rect = el.getBoundingClientRect()
    return {
      box: { width: el.clientWidth, height: el.clientHeight },
      picture: { width: picture?.offsetWidth ?? 0, height: picture?.offsetHeight ?? 0 },
      center: (x: number, y: number) => ({ x: x - rect.left - rect.width / 2, y: y - rect.top - rect.height / 2 }),
    }
  }, [])

  // Not a React handler: pinch arrives as wheel events, and the page must not zoom with it.
  useEffect(() => {
    const el = box.current
    if (!el) return
    const onWheel = (e: WheelEvent) => {
      e.preventDefault()
      const m = measure()
      if (e.ctrlKey) apply(zoomBy(current.current, pinchFactor(e.deltaY, e.deltaMode), m.center(e.clientX, e.clientY), m.box, m.picture), true)
      else if (current.current.scale > 1) apply(panBy(current.current, -e.deltaX, -e.deltaY, m.box, m.picture))
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  }, [apply, measure])

  // The window resizing leaves the picture where it was: keep it inside the new box.
  useEffect(() => {
    const el = box.current
    if (!el) return
    const observer = new ResizeObserver(() => {
      const m = measure()
      apply(clampZoom(current.current, m.box, m.picture))
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [apply, measure])

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.metaKey || e.ctrlKey || e.altKey) return
      // Typing elsewhere (the composer takes focus back while the viewer is still closing) is not for the picture.
      const target = e.target
      if (target instanceof HTMLElement && (target.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName))) return
      const factor = e.key === '+' || e.key === '=' ? KEY_STEP : e.key === '-' ? 1 / KEY_STEP : null
      if (factor === null && e.key !== '0') return
      e.preventDefault()
      const m = measure()
      apply(factor === null ? FIT : zoomBy(current.current, factor, { x: 0, y: 0 }, m.box, m.picture), true)
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  }, [apply, measure])

  const onPointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    touches.current.set(e.pointerId, { x: e.clientX, y: e.clientY })
  }
  const onPointerMove = (e: PointerEvent) => {
    if (!touches.current.has(e.pointerId)) return
    // A mouse button released outside the page (or a right-click menu) leaves no pointer-up.
    if (e.pointerType === 'mouse' && e.buttons === 0) return void touches.current.delete(e.pointerId)
    const before = [...touches.current.values()]
    touches.current.set(e.pointerId, { x: e.clientX, y: e.clientY })
    const after = [...touches.current.values()]
    const m = measure()
    if (after.length === 2) {
      const step = pinchStep(before as [Touch, Touch], after as [Touch, Touch])
      apply(panBy(zoomBy(current.current, step.factor, m.center(step.origin.x, step.origin.y), m.box, m.picture), step.dx, step.dy, m.box, m.picture), true)
    } else if (after.length === 1 && current.current.scale > 1) {
      apply(panBy(current.current, after[0].x - before[0].x, after[0].y - before[0].y, m.box, m.picture))
    }
  }
  const onPointerEnd = (e: PointerEvent) => void touches.current.delete(e.pointerId)

  return {
    ref: box,
    zoomed: zoom.scale > 1,
    style: { '--zoom': `translate(${zoom.x}px, ${zoom.y}px) scale(${zoom.scale})` } as CSSProperties,
    handlers: {
      onPointerDown,
      onPointerMove,
      onPointerUp: onPointerEnd,
      onPointerCancel: onPointerEnd,
      onDoubleClick: (e: React.MouseEvent) => {
        const m = measure()
        apply(toggleZoom(current.current, m.center(e.clientX, e.clientY), m.box, m.picture), true)
      },
    },
  }
}
