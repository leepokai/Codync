/** A short pause prevents passing over a row from changing the menu, and bridges the gap to its child. */
export const SUBMENU_HOVER_MS = 150

export function submenuHover(change: (index: number | null) => void) {
  let pending: ReturnType<typeof setTimeout> | undefined
  let target: number | null | undefined
  const cancel = () => {
    clearTimeout(pending)
    pending = undefined
    target = undefined
  }
  return {
    cancel,
    schedule(index: number | null) {
      if (pending && target === index) return
      cancel()
      target = index
      pending = setTimeout(() => {
        pending = undefined
        target = undefined
        change(index)
      }, SUBMENU_HOVER_MS)
    },
  }
}
