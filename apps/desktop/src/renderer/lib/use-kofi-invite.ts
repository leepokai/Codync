import { afterExit } from './exit-delay'
import { kofiInviteAt } from './kofi-invite'
import { prefs, usePref } from './prefs'
import { starAskedThisSession, useStarAsk } from './use-star-ask'
import { analytics } from '../store/analytics'
import type { ChatItem } from '../views/thread/chat-items'

export const KOFI_URL = 'https://ko-fi.com/leepokai'
/** The card's fade-out. */
export const KOFI_EXIT_MS = 200

/** Called for each prompt that arrived, before it's counted: the first one from this computer makes the invitation due. */
export function noteFirstPrompt(botId: string, since: number) {
  if (prefs.promptsSent.get() === 0 && prefs.kofiInvite.get().kind === 'never') prefs.kofiInvite.set({ kind: 'due', bot: botId, since })
}

/** The chat item the invitation sits under in `botId`'s chat; none while a GitHub star ask is due or after one in this session. */
export function useKofiInviteAt(botId: string, items: ChatItem[]): string | null {
  const [invite] = usePref(prefs.kofiInvite)
  const starDue = useStarAsk().due !== null
  return starDue || starAskedThisSession() ? null : kofiInviteAt(invite, botId, items)
}

/** The card appeared: counted once. */
export function noteKofiShown() {
  const invite = prefs.kofiInvite.get()
  if (invite.kind !== 'due' || invite.shown) return
  prefs.kofiInvite.set({ ...invite, shown: true })
  analytics.track({ event: 'kofi_invite_shown' })
}

/** *Support on Ko-fi* or dismiss: ends it for good once the card has faded out (the chat stops placing it then). */
export function answerKofiInvite(opened: boolean) {
  analytics.track({ event: 'kofi_invite_answered', opened })
  if (opened) window.codync.app.openExternal(KOFI_URL)
  afterExit(KOFI_EXIT_MS, () => prefs.kofiInvite.set({ kind: 'done' }))
}
