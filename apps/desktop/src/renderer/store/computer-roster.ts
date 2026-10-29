import { computerSections, moveInOrder, orderedComputers } from '../lib/computer-roster'
import { prefs, usePref } from '../lib/prefs'
import { useModel } from '../lib/observable'
import { foldsAfterRename, rosterLines, sameRosterGroup, sectionRename, shownRows, shownSection } from '../lib/roster-sections'
import type { BotStore } from './bot-store'
import type { AppModel, BotRef } from './app-model'

/** Device-local section ordering and folding, matching the iPhone roster. */
export function useComputerRoster(app: AppModel, shown: Set<string>) {
  const order = useModel(app.computerOrder).ids
  const [collapsed, setCollapsed] = usePref(prefs.collapsedComputers)
  const [folded, setFolded] = usePref(prefs.collapsedRosterSections)
  const computers = orderedComputers(app.computers, order)
  const sections = computerSections(computers.filter((c) => shown.has(c.id)), app.roster)
    .map((section) => ({ ...section, store: app.store(section.computer.id) }))
    .filter((section): section is typeof section & { store: BotStore } => section.store !== null)
  const grouped = sections.length > 1
  return {
    sections,
    grouped,
    collapsed,
    folded,
    items: sections.flatMap((section) => section.items),
    /** The rows on screen, top to bottom, for ↑/↓. */
    navigable: sections.flatMap((section) => grouped && collapsed.includes(section.computer.id) ? [] : shownRows(rosterLines(section.items, folded))),
    toggle(id: string) {
      setCollapsed(toggled(collapsed, id))
    },
    /** Folds or unfolds roster section `name` on this device. */
    toggleFold(name: string) {
      setFolded(toggled(folded, name))
    },
    /** Renames or removes (null) a section on `store`'s computer; a folded one stays folded under its new name, on this device, unless the host refuses. */
    renameSection(store: BotStore, from: string, to: string | null) {
      // Read now: a name dialog saves after the render that built its menu.
      const before = prefs.collapsedRosterSections.get()
      const after = foldsAfterRename(before, from, sectionRename(from, to).to)
      setFolded(after)
      // Refused: the folds go back with the bots (one render, no open-then-fold), unless they were toggled while the host answered.
      void store.sections.rename(from, to, () => {
        if (prefs.collapsedRosterSections.get() === after) setFolded(before)
      })
    },
    move(id: string, target: string) {
      app.computerOrder.move(id, target, computers.map((c) => c.id))
    },
    /** One place up (-1) or down (+1) among its computer's bots in the same band (lead, pinned, the rest) and, for the rest, the same section. */
    moveBot(ref: BotRef, offset: number) {
      const section = sections.find((s) => s.computer.id === ref.computerId)
      const items = section?.items ?? []
      const moved = items.find((item) => item.bot.id === ref.botId)
      const shown = moved && shownSection(moved.bot)
      // A folded section shows no order to change.
      if (!section || !moved || (shown && folded.includes(shown))) return
      const group = items.filter((item) => sameRosterGroup(item.bot, moved.bot))
      const target = group[group.indexOf(moved) + offset]
      if (target) section.store.reorder(moveInOrder(items.map((item) => item.bot.id), ref.botId, target.bot.id))
    },
  }
}

/** `list` with `value` added, or taken out when it's there. */
function toggled(list: string[], value: string) {
  return list.includes(value) ? list.filter((v) => v !== value) : [...list, value]
}
