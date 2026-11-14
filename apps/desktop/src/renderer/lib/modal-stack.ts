/** A page layer (a child of `<body>`), listed in page order. */
export interface PageLayer<T> {
  el: T
  z: number
  /** An open sheet or dialog. */
  modal: boolean
  inert: boolean
  /** Made inert by modals, so theirs to restore. */
  muted: boolean
}

/**
 * Which layers take clicks, focus and Escape while modals are open. Only the modal drawn on top
 * (highest z-index, the later one on a tie) does, plus the layers after it that aren't modals
 * (a menu opened from it); every other layer goes inert. Two modals can open on the same render
 * in either order, so this is recomputed on every open and close.
 */
export function stackModals<T>(layers: readonly PageLayer<T>[]) {
  const top = layers.reduce((best, layer, i) => (layer.modal && (best < 0 || layer.z >= layers[best].z) ? i : best), -1)
  const live = (layer: PageLayer<T>, i: number) => top < 0 || i === top || (i > top && !layer.modal)
  return {
    top: top < 0 ? null : layers[top].el,
    mute: layers.filter((layer, i) => !live(layer, i) && !layer.inert).map((layer) => layer.el),
    unmute: layers.filter((layer, i) => live(layer, i) && layer.muted).map((layer) => layer.el),
  }
}
