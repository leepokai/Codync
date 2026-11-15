import type { Bot } from '@shared/models'
import type { BotStore } from '../store/bot-store'
import type { MenuItem } from '../components/Menu'
import { makeLeadItems, offersDelete } from './lead-menus'
import { moveToSectionItems } from './section-menus'
import type { Naming } from './NameDialog'

/** What a bot row's menu opens in the window. */
export interface BotMenuActions {
  askName: (naming: Naming) => void
  edit: () => void
  confirmDelete: () => void
}

/** A bot row's menu: one plain list. The lead stays in sight: no hide. */
export function botActions(bot: Bot, store: BotStore, menu: BotMenuActions): MenuItem[] {
  return [
    { title: bot.pinned ? 'Unpin' : 'Pin', icon: 'pin', action: () => store.setPinned(bot, !bot.pinned) },
    ...moveToSectionItems(bot, store, menu.askName),
    store.offersMarkUnread(bot)
      ? { title: 'Mark as Unread', icon: 'record.circle', action: () => store.reads.markUnread(bot.id) }
      : { title: 'Mark as Read', icon: 'bell.badge', action: () => store.reads.markRead(bot.id) },
    { title: 'Edit Profile…', icon: 'square.and.pencil', action: menu.edit },
    { title: 'Copy conversation ID', icon: 'square.on.square', action: () => window.codync.app.copy(bot.id) },
    ...makeLeadItems(bot, store),
    ...(bot.lead ? [] : [{ title: 'Hide from sidebar', icon: 'eye.slash', action: () => store.setHidden(bot, true) }]),
    ...(offersDelete(bot, store) ? [{ title: 'Delete', icon: 'trash', destructive: true, action: menu.confirmDelete }] : []),
  ]
}
