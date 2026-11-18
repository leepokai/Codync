import { useEffect, useState } from 'react'
import { KOFI_ASK_DAYS, countDay, kofiAskAtOpening } from './kofi-ask'
import { prefs } from './prefs'
import { dueStarAsk } from './star-ask'
import { analytics } from '../store/analytics'

export const KOFI_URL = 'https://ko-fi.com/leepokai'

/** A prompt from this computer arrived: today is an active day. */
export function noteActiveDay() {
  const now = new Date()
  prefs.activeDays.set(countDay(prefs.activeDays.get(), `${now.getFullYear()}-${now.getMonth() + 1}-${now.getDate()}`))
}

/**
 * The invitation of one opening of the chat window, decided as it mounts (`held`: What's new is up). It counts once
 * on screen (`online`), answered or not; *Support on Ko-fi* ends them, *Not now* leaves the next one waiting.
 */
export function useKofiAsk(held: boolean, online: boolean) {
  const [ask] = useState(() =>
    kofiAskAtOpening(prefs.kofiAsk.get(), prefs.activeDays.get().count, {
      held,
      starDue: dueStarAsk(prefs.starAsk.get(), prefs.promptsSent.get()) !== null,
    }),
  )
  const [answered, setAnswered] = useState(false)
  const open = ask !== null && !answered && online
  useEffect(() => {
    if (!open || prefs.kofiAsk.get().asked >= ask) return
    prefs.kofiAsk.set({ asked: ask, day: prefs.activeDays.get().count })
    analytics.track({ event: 'kofi_invite_shown', ask })
  }, [open, ask])

  const answer = (opened: boolean) => {
    if (!open) return
    setAnswered(true)
    analytics.track({ event: 'kofi_invite_answered', ask, opened })
    if (!opened) return
    prefs.kofiAsk.set({ ...prefs.kofiAsk.get(), asked: KOFI_ASK_DAYS.length })
    window.codync.app.openExternal(KOFI_URL)
  }
  return { open, answer }
}
