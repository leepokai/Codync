import { useEffect, useRef, useState, type RefObject } from 'react'
import { isTruncated } from './truncation'

/**
 * A ref for a `truncate` element and whether it is clipping, kept true as the thing around it
 * changes size: the Bot Manager sheet is draggable and the main window resizable, so a path that
 * fitted a moment ago can clip now. `text` re-measures when the words change without the box
 * changing — a row's model was changed, so a longer path arrived.
 *
 * ResizeObserver is the way the app already watches a box (Controls, ChatLog, DecisionCard); nothing
 * polls this on a timer.
 */
export function useTruncated(text?: string | null): [RefObject<HTMLElement | null>, boolean] {
  const ref = useRef<HTMLElement | null>(null)
  const [truncated, setTruncated] = useState(false)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const measure = () => setTruncated(isTruncated(el.scrollWidth, el.clientWidth))
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(el)
    return () => observer.disconnect()
  }, [text])
  return [ref, truncated]
}
