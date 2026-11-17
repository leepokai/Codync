import { useEffect, useRef } from 'react'

type Layer = {
  ref: React.RefObject<HTMLElement | null>
  handler: React.RefObject<() => void>
}

// The layers still open, oldest first. Escape closes only the newest one: Find opened over the
// compact details takes the key, and the details the user was reading stay open.
const layers: Layer[] = []
let listening = false

/** The layer Escape belongs to: the newest one still on the page, not inert under a modal. */
export function newestLayer<T extends { ref: React.RefObject<HTMLElement | null> }>(open: readonly T[]): T | undefined {
  for (let at = open.length - 1; at >= 0; at--) {
    const layer = open[at]!
    const element = layer.ref.current
    if (element && !element.closest('[inert]')) return layer
  }
  return undefined
}

function onKeyDown(e: KeyboardEvent) {
  if (e.key !== 'Escape' || e.isComposing) return
  // Decided once the key has reached every listener: a menu that opened after this layer hears it
  // after this one, and takes the key first.
  setTimeout(() => {
    if (e.defaultPrevented) return
    newestLayer(layers)?.handler.current()
  })
}

/**
 * Escape closes the layer `ref` names wherever the focus is (its own field, the composer, the
 * chat), once nothing else took the key and no newer layer is open over it: a dialog or menu
 * opened over it (`useEscape` stops the key; the account panel and the composer's reply and `/`
 * list prevent its default), an input method's candidate, or a modal that made the page under it
 * inert.
 */
export function useEscapeLast(ref: React.RefObject<HTMLElement | null>, onEscape: () => void, active = true) {
  const handler = useRef(onEscape)
  handler.current = onEscape
  useEffect(() => {
    if (!active) return
    const layer: Layer = { ref, handler }
    layers.push(layer)
    if (!listening) {
      window.addEventListener('keydown', onKeyDown)
      listening = true
    }
    return () => {
      const at = layers.indexOf(layer)
      if (at >= 0) layers.splice(at, 1)
      if (!layers.length) {
        window.removeEventListener('keydown', onKeyDown)
        listening = false
      }
    }
  }, [ref, active])
}
