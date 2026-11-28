import type { Bot } from '@shared/models'
import type { Connection } from '../store/bot-store'
import { counted } from '../../shared/fill.ts'

type Ranked = Pick<Bot, 'pinned' | 'position' | 'lastAt'>

/** A bot's band in its computer's roster: pinned bots, then the rest. A drag moves a bot only within its band. */
export const rosterBand = (b: Pick<Bot, 'pinned'>) => (b.pinned ? 0 : 1)

/** Roster order: by band, then where the user dragged them, then most recent activity. */
export const rosterOrder = (a: Ranked, b: Ranked) => rosterBand(a) - rosterBand(b) || a.position - b.position || b.lastAt - a.lastAt

/** Reorder `ids` moving `from` to `to` while a frozen lead keeps its slot: the lead is lifted out for the
 * move and spliced back at its index, so a move across it never rewrites the lead's host position. */
export function movePreservingFrozenLead(ids: readonly string[], from: string, to: string, leadId: string | undefined): string[] {
  if (from === leadId || to === leadId) return [...ids]
  if (leadId === undefined) return moveInOrder([...ids], from, to)
  const moved = moveInOrder(ids.filter((id) => id !== leadId), from, to)
  moved.splice(ids.indexOf(leadId), 0, leadId)
  return moved
}

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

/** The roster header's line for the computers it shows; `waking`: one is a cloud computer on its way up (`isWaking`). */
export function connectionSummary(connections: Connection[], waking: boolean, tr: (text: string) => string) {
  if (!connections.length) return tr('Computers')
  const online = connections.filter((c) => c.kind === 'online').length
  if (online === connections.length) return counted(tr('%d connected'), online)
  if (online > 0) return counted(tr('%d/%d connected'), online, connections.length)
  if (connections.some((c) => c.kind === 'connecting')) return tr('Connecting…')
  if (waking) return tr('Waking the computer…')
  if (connections.every((c) => c.kind === 'unauthorized')) return tr('No access')
  if (connections.every((c) => c.kind === 'unpaired')) return tr('Not paired')
  return counted(tr('%d offline'), connections.length)
}
