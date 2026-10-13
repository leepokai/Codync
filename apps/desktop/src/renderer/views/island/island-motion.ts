import { useEffect, useLayoutEffect, useRef, useState, type RefObject } from 'react'
import { reduceMotion } from '../../lib/theme'

// Switching bots animates: the slot that shows one bot plays `island-swap` when another bot takes
// it, and a bot joining a row of bots plays it as it arrives. Neither plays when a view first
// appears (the view's own entrance covers that).

/**
 * The class for an element keyed by `id` (give it `key={id}` so a new bot mounts a new element):
 * `island-swap` once the id changed, so each new bot animates in.
 */
export function useSwap(id: string | null | undefined) {
  const last = useRef<{ id: string | null | undefined; animate: boolean } | null>(null)
  if (!last.current) last.current = { id, animate: false }
  else if (last.current.id !== id) last.current = { id, animate: true }
  return last.current.animate ? 'island-swap' : ''
}

/** The ids that just joined a list of bots (none on its first render), to animate in. */
export function useArrivals(ids: string[]) {
  const seen = useRef<Set<string> | null>(null)
  const arrivals = useRef<ReadonlySet<string>>(new Set())
  const key = ids.join('\n')
  const lastKey = useRef<string | null>(null)
  if (lastKey.current !== key) {
    const before = seen.current
    arrivals.current = before ? new Set(ids.filter((id) => !before.has(id))) : new Set()
    seen.current = new Set(ids)
    lastKey.current = key
  }
  return arrivals.current
}

/**
 * The view that `value` just replaced, kept for `ms` so both render and a tab switch
 * cross-fades instead of cutting. Null when nothing is leaving.
 */
export function useCrossFade<T>(value: T, ms: number): T | null {
  const [state, setState] = useState<{ value: T; previous: T | null }>({ value, previous: null })
  if (state.value !== value) setState({ value, previous: state.value })
  useEffect(() => {
    if (state.previous === null) return
    const timer = setTimeout(() => setState((s) => ({ ...s, previous: null })), ms)
    return () => clearTimeout(timer)
  }, [state.previous, ms])
  return state.previous !== value ? state.previous : null
}

/**
 * Bots that move within a row slide to their new place (FLIP): elements under `container`
 * marked `data-flip` (their ref) are measured after every render and eased from where they
 * were. Layout offsets, so the views' own scale animations never read as a move.
 */
export function useFlip(container: RefObject<HTMLElement | null>) {
  const places = useRef(new Map<string, { x: number; y: number }>())
  useLayoutEffect(() => {
    const next = new Map<string, { x: number; y: number }>()
    for (const el of container.current?.querySelectorAll<HTMLElement>('[data-flip]') ?? []) {
      const id = el.dataset.flip ?? ''
      const place = { x: el.offsetLeft, y: el.offsetTop }
      const before = places.current.get(id)
      next.set(id, place)
      if (!before || (before.x === place.x && before.y === place.y) || reduceMotion()) continue
      el.animate([{ transform: `translate(${before.x - place.x}px, ${before.y - place.y}px)` }, { transform: 'none' }], {
        duration: 420,
        easing: 'cubic-bezier(0.32, 1.22, 0.42, 1)',
      })
    }
    places.current = next
  })
}
