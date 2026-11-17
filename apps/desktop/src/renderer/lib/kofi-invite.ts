import type { ChatItem } from '../views/thread/chat-items'

/**
 * The Ko-fi invitation on this computer: due once the first prompt counted on it (`prefs.promptsSent`, counted since
 * 2.39.1) arrived, in `bot`'s chat at `since` (`shown` once the card was counted); done once opened or dismissed.
 * Desktop and terminal only: the stores' rules on external tip links keep it off the phones.
 */
export type KofiInvite = { kind: 'never' } | { kind: 'due'; bot: string; since: number; shown?: boolean } | { kind: 'done' }

/** The chat item the invitation sits under: the first finished reply in its chat after the first prompt. */
export function kofiInviteAt(invite: KofiInvite, botId: string, items: ChatItem[]): string | null {
  if (invite.kind !== 'due' || invite.bot !== botId) return null
  const reply = items.find((i) => i.kind === 'entry' && i.entry.kind === 'agent' && i.entry.data.final === true && i.entry.createdAt >= invite.since)
  return reply?.id ?? null
}
