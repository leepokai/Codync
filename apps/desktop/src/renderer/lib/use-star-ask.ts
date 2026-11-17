import { prefs, usePref } from './prefs'
import { afterStarAsk, dueStarAsk } from './star-ask'
import { analytics } from '../store/analytics'

export const STAR_REPO = 'https://github.com/leepokai/Codync'

/** A star ask (setup's panel or the invitation) was answered in this window: the Ko-fi invitation waits for a later one. */
let answeredThisSession = false
export const starAskedThisSession = () => answeredThisSession

/** Setup and the invitation at the fifth prompt share one persisted answer history. */
export function useStarAsk() {
  const [state, setState] = usePref(prefs.starAsk)
  const [prompts] = usePref(prefs.promptsSent)
  const answer = (starred: boolean) => {
    answeredThisSession = true
    const current = prefs.starAsk.get()
    const sent = prefs.promptsSent.get()
    const due = dueStarAsk(current, sent)
    setState(afterStarAsk(current, starred, sent))
    if (due) {
      analytics.track({ event: 'star_prompt_answered', ask: due, starred })
    }
    if (starred) window.codync.app.openExternal(STAR_REPO)
  }
  return { due: dueStarAsk(state, prompts), prompts, answer }
}
