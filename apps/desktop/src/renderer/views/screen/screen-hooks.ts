import { useEffect, useLayoutEffect, useState, useSyncExternalStore, type RefObject } from 'react'
import type { ScreenViewerModel } from './screen-viewer-model'

/** Whether a Remote screen window of this computer is open (the preview in the details panel yields to it). */
export function useScreenWindowOpen(computerId: string): boolean {
  const [open, setOpen] = useState(false)
  useEffect(() => {
    let current = true
    // A change that lands before the list answers is newer than the list.
    let heard = false
    const unsubscribe = window.codync.screenViewer.onWindowsChanged((ids) => {
      heard = true
      setOpen(ids.includes(computerId))
    })
    void window.codync.screenViewer.openWindows().then((ids) => { if (current && !heard) setOpen(ids.includes(computerId)) }, () => {})
    return () => {
      current = false
      unsubscribe()
    }
  }, [computerId])
  return open
}

const subscribeHidden = (notify: () => void) => {
  document.addEventListener('visibilitychange', notify)
  return () => document.removeEventListener('visibilitychange', notify)
}

/** This app window is hidden or minimized. */
export function useDocumentHidden(): boolean {
  return useSyncExternalStore(subscribeHidden, () => document.hidden)
}

/** The video box's size and the device pixel ratio, kept on the model: they place the video and the pointer. */
export function useStageSize(stage: RefObject<HTMLDivElement | null>, model: ScreenViewerModel | null) {
  useLayoutEffect(() => {
    const element = stage.current
    if (!element || !model) return
    const measure = () => {
      const { width, height } = element.getBoundingClientRect()
      model.setView({ width, height }, window.devicePixelRatio || 1)
    }
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    // Moving the window to another display changes the ratio without resizing it.
    let query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
    const ratio = () => {
      measure()
      query.removeEventListener('change', ratio)
      query = matchMedia(`(resolution: ${window.devicePixelRatio}dppx)`)
      query.addEventListener('change', ratio)
    }
    query.addEventListener('change', ratio)
    window.addEventListener('resize', measure)
    return () => {
      observer.disconnect()
      query.removeEventListener('change', ratio)
      window.removeEventListener('resize', measure)
    }
  }, [stage, model])
}
