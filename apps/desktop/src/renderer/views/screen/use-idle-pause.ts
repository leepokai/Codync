import { useEffect } from 'react'
import { untouchedTooLong } from '../../lib/screen-state'
import { analytics } from '../../store/analytics'

/** What counts as touching the app: the pointer, a key, the wheel (watching alone doesn't). */
const TOUCHES = ['pointermove', 'pointerdown', 'keydown', 'wheel'] as const
const CHECK_MS = 10_000

/**
 * Calls `pause` once a live picture has gone `SCREEN_IDLE_MS` with nobody touching this window, counted from when it
 * went live (connecting or failed sessions send nothing, so they never pause). `screen` names it in
 * `remote_screen_paused`.
 */
export function usePauseWhenIdle(live: boolean, pause: () => void, screen: 'bot_computer' | 'remote_screen') {
  useEffect(() => {
    if (!live) return
    let touched = Date.now()
    const touch = () => {
      touched = Date.now()
    }
    for (const name of TOUCHES) window.addEventListener(name, touch, { capture: true, passive: true })
    const timer = setInterval(() => {
      if (!untouchedTooLong(touched, Date.now())) return
      analytics.track({ event: 'remote_screen_paused', screen })
      pause()
    }, CHECK_MS)
    return () => {
      clearInterval(timer)
      for (const name of TOUCHES) window.removeEventListener(name, touch, { capture: true })
    }
  }, [live])
}
