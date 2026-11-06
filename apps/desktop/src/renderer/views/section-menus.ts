import type { Bot } from '@shared/models'
import type { MenuItem } from '../components/Overlay'
import { sectionNames } from '../lib/roster-sections.ts'
import type { BotStore } from '../store/bot-store'
import type { ActionItem } from './DesktopActionMenu'

/** An open section name dialog: its title, the name it starts with, and what Save does with the typed one. */
export interface SectionNaming {
  title: string
  name: string
  save: (name: string) => void
}

/** A bot row's "Move to section"; none while its computer runs a host without sections. */
export function moveToSectionItems(bot: Bot, store: BotStore, askName: (naming: SectionNaming) => void): ActionItem[] {
  if (!store.hasRosterSections) return []
  const others = sectionNames(store.bots.values()).filter((name) => name !== bot.section)
  const submenu: MenuItem[] = [
    ...others.map((name) => ({ title: name, icon: 'tray', verbatim: true, action: () => store.sections.move(bot, name) })),
    { title: 'New section…', icon: 'plus', divider: others.length > 0, action: () => askName({ title: 'New section', name: '', save: (name) => store.sections.move(bot, name) }) },
    ...(bot.section ? [{ title: 'Remove from section', icon: 'minus.circle', action: () => store.sections.move(bot, null) }] : []),
  ]
  return [{ title: 'Move to section', icon: 'tray', submenu }]
}

/** A section header's right-click menu, `rename` taking the new name (null: remove it, its bots staying without one). None while the computer runs a host without sections. */
export function sectionHeaderItems(section: string, store: BotStore, askName: (naming: SectionNaming) => void, rename: (to: string | null) => void): MenuItem[] {
  if (!store.hasRosterSections) return []
  return [
    { title: 'Rename section…', icon: 'pencil', action: () => askName({ title: 'Rename section', name: section, save: rename }) },
    { title: 'Remove section', icon: 'xmark.circle', action: () => rename(null) },
  ]
}
