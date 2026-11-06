import type { Bot } from '@shared/models'

type Ranked = Pick<Bot, 'pinned' | 'position' | 'lastAt'>

/** A bot's band in its computer's roster: pinned bots, then the rest. A drag moves a bot only within its band. */
export const rosterBand = (b: Pick<Bot, 'pinned'>) => (b.pinned ? 0 : 1)

/** Roster order: by band, then where the user dragged them, then most recent activity. */
export const rosterOrder = (a: Ranked, b: Ranked) => rosterBand(a) - rosterBand(b) || a.position - b.position || b.lastAt - a.lastAt

/** Saved computers retain their order; newly connected computers follow them. */
export function orderedComputers<T extends { id: string }>(computers: T[], order: string[]): T[] {
  const ranks = new Map(order.map((id, index) => [id, index]))
  return [...computers].sort((a, b) => (ranks.get(a.id) ?? order.length) - (ranks.get(b.id) ?? order.length))
}

/** Insert at the target's position, preserving hidden entries and unrelated ordering (computers, bots). */
export function moveInOrder(order: string[], id: string, target: string): string[] {
  const from = order.indexOf(id)
  const to = order.indexOf(target)
  if (from < 0 || to < 0 || from === to) return order
  const next = [...order]
  next.splice(from, 1)
  next.splice(to, 0, id)
  return next
}

/** Group without changing pinned/activity order inside each computer. Include empty computers. */
export function computerSections<C extends { id: string }, T extends { ref: { computerId: string } }>(computers: C[], roster: T[]) {
  const sections = computers.map((computer) => ({ computer, items: [] as T[] }))
  const byID = new Map(sections.map((section) => [section.computer.id, section]))
  for (const item of roster) byID.get(item.ref.computerId)?.items.push(item)
  return sections
}
