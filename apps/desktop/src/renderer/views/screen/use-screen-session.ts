import { useEffect, useState, useSyncExternalStore } from 'react'
import { useModel } from '../../lib/observable'
import { displayToShow, type ScreenComputer } from '../../lib/screen-state'
import type { BotStore } from '../../store/bot-store'
import { ScreenSession } from '../../store/screen-session'

/** The computer as its picture's notices see it. */
export function screenComputer(store: BotStore): ScreenComputer {
  const kind = store.shownConnection.kind
  return { screen: store.screen, asleep: store.isAsleep, connection: kind === 'online' ? 'online' : kind === 'connecting' ? 'connecting' : 'offline' }
}

/** One session while the screen can be watched; it closes when this does or the screen goes away. A new one shows
 * the display `picked` names (the viewer's choice, kept across sessions), else the main one. */
export function useScreenSession(store: BotStore, available: boolean, picked?: React.RefObject<number | null>) {
  const [session, setSession] = useState<ScreenSession | null>(null)
  useEffect(() => {
    if (!available) return
    let live: ScreenSession | null = null
    let cancelled = false
    void store.ready().then((client) => {
      const display = store.screen && displayToShow(store.screen, picked?.current ?? null)
      if (cancelled || !display) return
      live = new ScreenSession(client, store.hostRoute === 'relay', display)
      setSession(live)
      void live.start()
    }, () => {})
    return () => {
      cancelled = true
      live?.close()
      setSession(null)
    }
  }, [store, available])
  return useModel(session)
}

/** Tells the computer how big the picture is shown, so it sends no more pixels than that. */
export function useViewSize(area: React.RefObject<HTMLElement | null>, session: ScreenSession | null) {
  const live = session?.phase.kind === 'live'
  useEffect(() => {
    const element = area.current
    if (!element || !session || !live) return
    let timer: ReturnType<typeof setTimeout> | undefined
    const report = () => {
      const scale = window.devicePixelRatio || 1
      const w = element.clientWidth * scale
      const h = element.clientHeight * scale
      session.view(Math.max(w, h), Math.min(w, h))
    }
    const observer = new ResizeObserver(() => {
      clearTimeout(timer)
      timer = setTimeout(report, 200)
    })
    observer.observe(element)
    report()
    return () => {
      clearTimeout(timer)
      observer.disconnect()
    }
  }, [area, session, live])
}

/** The session's picture in `video`. */
export function useVideo(video: React.RefObject<HTMLVideoElement | null>, session: ScreenSession | null) {
  useEffect(() => {
    if (video.current) video.current.srcObject = session?.video ?? null
  }, [video, session?.video])
}

const onVisibility = (cb: () => void) => {
  document.addEventListener('visibilitychange', cb)
  return () => document.removeEventListener('visibilitychange', cb)
}

/** Whether the window shows: hidden (closed to the background) or minimized, a picture lets its stream go. */
export function useWindowShown() {
  return useSyncExternalStore(onVisibility, () => document.visibilityState === 'visible')
}
