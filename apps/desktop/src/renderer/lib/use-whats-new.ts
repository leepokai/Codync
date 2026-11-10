import { useEffect, useRef, useState } from 'react'
import { prefs } from './prefs'
import { announcements, due as dueFor } from './whats-new'
import { analytics } from '../store/analytics'

/**
 * This launch's What's new, decided when the chat window opens: before the welcome, so a first run that finishes it
 * still counts as a new install. Shown once `ready` (past the welcome and setup) until closed. The current version
 * is recorded at once when nothing is due, otherwise once the window is on screen.
 */
export function useWhatsNew(ready: boolean) {
  const [due] = useState(() => dueFor(announcements, prefs.whatsNewSeen.get(), window.codync.appVersion, prefs.onboardingDone.get()))
  const [closed, setClosed] = useState(false)
  const open = ready && due.length > 0 && !closed
  const recorded = useRef(false)
  useEffect(() => {
    if (recorded.current || (due.length > 0 && !open)) return
    recorded.current = true
    prefs.whatsNewSeen.set(window.codync.appVersion)
    if (open) analytics.track({ event: 'whats_new_shown' })
  }, [open, due])
  return { open, due, close: () => setClosed(true) }
}
