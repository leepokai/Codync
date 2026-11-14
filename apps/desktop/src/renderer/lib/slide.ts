import { useEffect, useLayoutEffect, useRef, type RefObject } from 'react'
import { reduceMotion } from './theme'

/** Where an element sits inside its parent; relative to it, so a row doesn't slide again inside a section that is already sliding. */
function topInParent(element: HTMLElement, parent: HTMLElement) {
  return element.offsetTop - (parent.offsetParent === element.offsetParent ? parent.offsetTop : 0)
}

/** A fold or unfold is moving things around: that is not a reorder. */
function folding(parent: HTMLElement) {
  return parent.getAnimations({ subtree: true }).some((a) => a instanceof CSSTransition && a.transitionProperty === 'grid-template-rows')
}

/** Slides an element from where it was to where it is now inside its parent (reordered rows and sections). */
export function useSlide(ref: RefObject<HTMLElement | null>) {
  const lastTop = useRef<number | null>(null)
  useLayoutEffect(() => {
    const element = ref.current
    const parent = element?.parentElement
    if (!element || !parent) return
    const top = topInParent(element, parent)
    if (lastTop.current !== null && top !== lastTop.current && !folding(parent) && !reduceMotion()) {
      element.animate([{ transform: `translateY(${lastTop.current - top}px)` }, { transform: 'translateY(0)' }], { duration: 300, easing: 'ease-out' })
    }
    lastTop.current = top
  })
  // A fold ends without a render: take where it left the element, so the next render doesn't slide from before it.
  useEffect(() => {
    const element = ref.current
    const parent = element?.parentElement
    if (!element || !parent) return
    const settle = () => {
      lastTop.current = topInParent(element, parent)
    }
    parent.addEventListener('transitionend', settle)
    return () => parent.removeEventListener('transitionend', settle)
  }, [ref])
}
