import { useLayoutEffect, useRef, type RefObject } from 'react'
import { reduceMotion } from './theme'

/** Slides an element from where it was to where it is now inside its parent (reordered rows and sections). */
export function useSlide(ref: RefObject<HTMLElement | null>) {
  const lastTop = useRef<number | null>(null)
  useLayoutEffect(() => {
    const element = ref.current
    const parent = element?.parentElement
    if (!element || !parent) return
    // Relative to the parent, so a row doesn't slide again inside a section that is already sliding.
    const top = element.offsetTop - (parent.offsetParent === element.offsetParent ? parent.offsetTop : 0)
    if (lastTop.current !== null && top !== lastTop.current && !reduceMotion()) {
      element.animate([{ transform: `translateY(${lastTop.current - top}px)` }, { transform: 'translateY(0)' }], { duration: 300, easing: 'ease-out' })
    }
    lastTop.current = top
  })
}
