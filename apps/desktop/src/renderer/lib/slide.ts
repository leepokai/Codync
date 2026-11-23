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

/** The theme's `--spring` (settles without bouncing) and how long a moved row takes to land. */
const SPRING = 'linear(0, 0.0712, 0.2244, 0.3992, 0.5638, 0.7018, 0.8077, 0.8834, 0.9342, 0.9663, 0.9853, 0.9958, 1.0009, 1.0028, 1.0029, 1)'
const SLIDE_MS = 420

/** Slides an element from where it was to where it is now inside its parent (reordered rows and sections). */
export function useSlide(ref: RefObject<HTMLElement | null>) {
  const lastTop = useRef<number | null>(null)
  useLayoutEffect(() => {
    const element = ref.current
    const parent = element?.parentElement
    if (!element || !parent) return
    const top = topInParent(element, parent)
    if (lastTop.current !== null && top !== lastTop.current && !folding(parent) && !reduceMotion()) {
      element.animate([{ transform: `translateY(${lastTop.current - top}px)` }, { transform: 'translateY(0)' }], { duration: SLIDE_MS, easing: SPRING })
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

/** Where each `[data-flip]` row of `list` sits, by its id, down the list's scrolled content. */
function rowTops(list: HTMLElement) {
  const top = list.getBoundingClientRect().top - (list.querySelector('.roster')?.scrollTop ?? 0)
  return new Map([...list.querySelectorAll<HTMLElement>('[data-flip]')].map((row) => [row, row.getBoundingClientRect().top - top] as const))
}

/** When a list's `layout` changes (grouped or sorted another way), slides every `[data-flip]` row of `list` from where
 * it was to where it is now, matched by its id across the whole list since a row that changes group is a new
 * element; a row that wasn't there fades in. A new `scope` (another list) moves nothing. */
export function useListFlip(list: RefObject<HTMLElement | null>, scope: string, layout: string) {
  const last = useRef<{ scope: string; layout: string; tops: Map<string, number> } | null>(null)
  useLayoutEffect(() => {
    const element = list.current
    if (!element) return
    const was = last.current
    const changed = was !== null && was.scope === scope && was.layout !== layout && !reduceMotion()
    // The rows' own slides start from where the old layout had them: this one slide replaces them.
    if (changed) for (const row of element.querySelectorAll<HTMLElement>('[data-flip]')) for (const a of row.getAnimations()) a.cancel()
    const tops = new Map([...rowTops(element)].map(([row, top]) => [row.dataset.flip!, top] as const))
    if (changed) {
      for (const [row, top] of rowTops(element)) {
        const from = was.tops.get(row.dataset.flip!)
        if (from === undefined) row.animate([{ opacity: 0 }, { opacity: 1 }], { duration: SLIDE_MS, easing: 'ease-out' })
        else if (from !== top) row.animate([{ transform: `translateY(${from - top}px)` }, { transform: 'translateY(0)' }], { duration: SLIDE_MS, easing: SPRING })
      }
    }
    last.current = { scope, layout, tops }
  })
}
