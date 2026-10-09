import { computerSections, orderedComputers } from '../lib/computer-roster'
import { prefs, usePref } from '../lib/prefs'
import { useModel } from '../lib/observable'
import type { BotStore } from './bot-store'
import type { AppModel } from './app-model'

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
  }
}
