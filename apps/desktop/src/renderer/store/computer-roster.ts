import { computerSections, moveInOrder, orderedComputers } from '../lib/computer-roster'
import { prefs, usePref } from '../lib/prefs'
import { useModel } from '../lib/observable'
import type { BotStore } from './bot-store'
import type { AppModel, BotRef } from './app-model'

/** Device-local section ordering and folding, matching the iPhone roster. */
export function useComputerRoster(app: AppModel, shown: Set<string>) {
  const order = useModel(app.computerOrder).ids
  const [collapsed, setCollapsed] = usePref(prefs.collapsedComputers)
  const computers = orderedComputers(app.computers, order)
  const sections = computerSections(computers.filter((c) => shown.has(c.id)), app.roster)
    .map((section) => ({ ...section, store: app.store(section.computer.id) }))
    .filter((section): section is typeof section & { store: BotStore } => section.store !== null)
  const grouped = sections.length > 1
  return {
    sections,
    grouped,
    collapsed,
    items: sections.flatMap((section) => section.items),
    navigable: sections.flatMap((section) => grouped && collapsed.includes(section.computer.id) ? [] : section.items),
    toggle(id: string) {
      setCollapsed(collapsed.includes(id) ? collapsed.filter((value) => value !== id) : [...collapsed, id])
    },
    move(id: string, target: string) {
      app.computerOrder.move(id, target, computers.map((c) => c.id))
    },
    /** One place up (-1) or down (+1) among its computer's bots with the same pin state. */
    moveBot(ref: BotRef, offset: number) {
      const section = sections.find((s) => s.computer.id === ref.computerId)
      const items = section?.items ?? []
      const index = items.findIndex((item) => item.bot.id === ref.botId)
      const target = items[index + offset]
      if (!section || index < 0 || !target || target.bot.pinned !== items[index]!.bot.pinned) return
      section.store.reorder(moveInOrder(items.map((item) => item.bot.id), ref.botId, target.bot.id))
    },
  }
}
