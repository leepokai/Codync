import { useLayoutEffect, useRef, type UIEvent } from 'react'
import { scrolledToEnd } from './trace-turns'

/** A scroll box that opens at the end and follows new content there until the reader scrolls up; `toEnd` brings it
 * back (a message the reader sends). */
export function useFollowEnd() {
  const ref = useRef<HTMLDivElement>(null)
  const atEnd = useRef(true)
  useLayoutEffect(() => {
    const el = ref.current
    if (el && atEnd.current) el.scrollTop = el.scrollHeight
  })
  const onScroll = (e: UIEvent<HTMLDivElement>) => {
    atEnd.current = scrolledToEnd(e.currentTarget)
  }
  const toEnd = () => {
    atEnd.current = true
  }
  return { ref, onScroll, toEnd }
}
