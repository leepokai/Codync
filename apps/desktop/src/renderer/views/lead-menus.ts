import { isGroup, type Bot } from '../../shared/models.ts'
import type { MenuItem } from '../components/Overlay'
import type { BotStore } from '../store/bot-store'

/** A bot row's "Make lead agent"; none for the lead, a group, or while its computer runs a host where the lead can't change. */
export function makeLeadItems(bot: Bot, store: BotStore): MenuItem[] {
  if (bot.lead || isGroup(bot) || !store.hasLeadChoice) return []
  return [{ title: 'Make lead agent', icon: 'star', action: () => store.setLead(bot) }]
}

/** Whether a bot row offers Delete: an older host keeps its lead. */
export function offersDelete(bot: Bot, store: BotStore): boolean {
  return !bot.lead || store.hasLeadChoice
}

/** Who the voice shortcut talks to when no chat is open: the lead, or the top bot where no computer has one. */
export function voiceTarget<T extends { bot: Bot }>(roster: { items: T[]; navigable: T[] }): T | undefined {
  return roster.items.find((i) => i.bot.lead) ?? roster.navigable[0] ?? roster.items[0]
}
