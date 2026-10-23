import { useEffect, useLayoutEffect, useRef, type RefObject } from 'react'
import { reduceMotion } from './theme'

/** Slides an element from where it was to where it is now inside its parent (reordered rows and sections). */
export function useSlide(ref: RefObject<HTMLElement | null>) {
  const lastTop = useRef<number | null>(null)
  useLayoutEffect(() => {
    const element = ref.current
    const top = element && topInParent(element)
    if (!element || top === null) return
    if (lastTop.current !== null && top !== lastTop.current && !reduceMotion()) {
      element.animate([{ transform: `translateY(${lastTop.current - top}px)` }, { transform: 'translateY(0)' }], { duration: 300, easing: 'ease-out' })
    }
    lastTop.current = top
  })
  // A fold moves the rows below it after its render, with none of its own: where they settle is their place.
  useEffect(() => {
    const settle = (event: TransitionEvent) => {
      const top = event.propertyName === 'grid-template-rows' && ref.current ? topInParent(ref.current) : null
      if (top !== null) lastTop.current = top
    }
    document.addEventListener('transitionend', settle)
    return () => document.removeEventListener('transitionend', settle)
  }, [ref])
}

/** Relative to the parent, so a row doesn't slide again inside a section that is already sliding. */
function topInParent(element: HTMLElement): number | null {
  const parent = element.parentElement
  if (!parent) return null
  return element.offsetTop - (parent.offsetParent === element.offsetParent ? parent.offsetTop : 0)
}
