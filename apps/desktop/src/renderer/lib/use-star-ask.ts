import { prefs, usePref } from './prefs'
import { afterStarAsk, dueStarAsk } from './star-ask'
import { analytics } from '../store/analytics'

export const STAR_REPO = 'https://github.com/leepokai/Codync'

/** Setup and the later reminder share one persisted answer history. */
export function useStarAsk() {
  const [state, setState] = usePref(prefs.starAsk)
  const answer = (starred: boolean) => {
    const current = prefs.starAsk.get()
    const now = Date.now()
    const due = dueStarAsk(current, now)
    setState(afterStarAsk(current, starred, now))
    if (due) {
      analytics.track({ event: 'star_prompt_answered', ask: due, starred })
    }
    if (starred) window.codync.app.openExternal(STAR_REPO)
  }
  return { state, answer }
}
