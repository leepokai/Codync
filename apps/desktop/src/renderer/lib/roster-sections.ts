import { isBelow } from '../../shared/compat.ts'
import { orderedSections } from './roster-section-order.ts'
import { rosterBand } from './computer-roster.ts'

/** The first host with roster sections. */
const ROSTER_SECTIONS_HOST = '2.32.0'

/** Whether a host of `version` keeps roster sections (an older one leaves them out). */
export function hostHasSections(version: string): boolean {
  return !isBelow(version, ROSTER_SECTIONS_HOST)
}

/** What placing a bot in the roster reads: its pin, the section the user put it in, its unread count. */
interface Placed {
  pinned: boolean
  section?: string
  unread: number
}

/** One line of a computer's roster: a bot's row, a section header holding its rows, or the "Bots" label. */
export type RosterLine<T> =
  | { kind: 'row'; item: T }
  | { kind: 'section'; name: string; count: number; unread: number; folded: boolean; items: T[] }
  | { kind: 'label' }

/**
 * A computer's roster (already in roster order, hidden bots left out) as lines: the pinned rows, then
 * each section by name with its rows, then rows without a section, under a "Bots" label when there are
 * sections. A pinned bot shows only on top. A folded section still holds its rows, so its fold can
 * animate.
 */
export function rosterLines<T extends { bot: Placed }>(items: T[], folded: readonly string[], order: readonly string[] = []): RosterLine<T>[] {
  const row = (item: T): RosterLine<T> => ({ kind: 'row', item })
  const groups = rosterGroups(items, folded, order)
  const labeled = groups.some((group) => group.name !== '')
  return [...items.filter((item) => item.bot.pinned).map(row), ...groups.flatMap((group): RosterLine<T>[] =>
    group.name !== '' ? [group] : [...(labeled ? [{ kind: 'label' } as const] : []), ...group.items.map(row)])]
}

/** The same whole groups for rendering, keyboard navigation, dragging, and renaming. */
export function rosterGroups<T extends { bot: Placed }>(items: T[], folded: readonly string[], order: readonly string[] = []): Extract<RosterLine<T>, { kind: 'section' }>[] {
  const unpinned = items.filter((item) => !item.bot.pinned)
  const names = [...sectionNames(unpinned.map((item) => item.bot)), ...(unpinned.some((item) => !item.bot.section) ? [''] : [])]
  return orderedSections(names, order).map((name) => {
    const rows = unpinned.filter((item) => (item.bot.section ?? '') === name)
    return { kind: 'section', name, count: rows.length, unread: rows.reduce((sum, item) => sum + item.bot.unread, 0), folded: name !== '' && folded.includes(name), items: rows }
  })
}

/** The rows on screen, top to bottom: a folded section's rows are left out. */
export function shownRows<T>(lines: RosterLine<T>[]): T[] {
  return lines.flatMap((line) => (line.kind === 'row' ? [line.item] : line.kind === 'label' || line.folded ? [] : line.items))
}

/** The section a bot shows in: none for pinned bots, which stay on top (terminal `Bot::shown_section`). */
export function shownSection(bot: Omit<Placed, 'unread'>): string | undefined {
  return bot.pinned ? undefined : bot.section || undefined
}

/** Whether a drag (or Alt+↑/↓) may move one bot past another: the same band (pinned, the rest) and the same section shown. */
export function sameRosterGroup(a: Omit<Placed, 'unread'>, b: Omit<Placed, 'unread'>): boolean {
  return rosterBand(a) === rosterBand(b) && shownSection(a) === shownSection(b)
}

/** Every section among `bots` once, by name as the user's language sorts it ("İş" too; "work" before "Work", as on the phones). */
export function sectionNames(bots: Iterable<{ section?: string }>): string[] {
  const names = new Set<string>()
  for (const bot of bots) if (bot.section) names.add(bot.section)
  return [...names].sort((a, b) => a.localeCompare(b))
}

/** The section a typed name names: trimmed of whitespace and control characters, null when nothing is left (iOS `RosterSection.name(typed:)`). */
export function sectionName(typed: string | null): string | null {
  return typed?.replace(/^[\s\p{Cc}]+|[\s\p{Cc}]+$/gu, '') || null
}

/** `renameSection`'s payload: `to` as `sectionName` reads it, and null (sent, not left out) to remove the section. */
export function sectionRename(from: string, to: string | null): { from: string; to: string | null } {
  return { from, to: sectionName(to) }
}

/**
 * This device's folded sections after it renames section `from` to `to` (null: removes it): a folded
 * `from` stays folded under `to`, so a merge folds the section it joins; an unfolded one changes nothing.
 */
export function foldsAfterRename(folded: readonly string[], from: string, to: string | null): string[] {
  if (!folded.includes(from)) return [...folded]
  const rest = folded.filter((name) => name !== from)
  return to === null || rest.includes(to) ? rest : [...rest, to]
}

/** The ids of the bots in section `name`: what renaming or removing it moves, and a refusal moves back (never bots already in the target). */
export function sectionMembers(bots: Iterable<{ id: string; section?: string }>, name: string): string[] {
  return [...bots].filter((bot) => bot.section === name).map((bot) => bot.id)
}

/** The bots with these ids still there, each put in `section` (none: undefined) and otherwise as they are now: a section change shown before the host confirms it. */
export function placedBots<T extends { section?: string }>(bots: ReadonlyMap<string, T>, ids: readonly string[], section: string | undefined): [string, T][] {
  return ids.flatMap((id): [string, T][] => {
    const bot = bots.get(id)
    return bot ? [[id, { ...bot, section }]] : []
  })
}

/** A section change shown before the host accepted it: bot `id` from section `from` to `to` (none: undefined). */
export interface SectionMove {
  id: string
  from: string | undefined
  to: string | undefined
}

/**
 * Taking back `moves` the host refused: each bot still in the section its move put it in goes back to
 * the one it left, otherwise as it is now. A bot moved again since (here or on another device) keeps
 * its newer section, and a deleted one stays deleted.
 */
export function restoredBots<T extends { section?: string }>(bots: ReadonlyMap<string, T>, moves: readonly SectionMove[]): [string, T][] {
  return moves.flatMap(({ id, from, to }): [string, T][] => {
    const bot = bots.get(id)
    return bot && bot.section === to ? [[id, { ...bot, section: from }]] : []
  })
}

/**
 * The moves still waiting (`pending`, oldest first) that started where `refused` put its bot: later
 * moves of the same bot from its `to`. They start where `refused` began instead, so taking one of
 * them back too lands where the host still has the bot.
 */
export function chainedMoves(pending: readonly SectionMove[], refused: SectionMove): SectionMove[] {
  return pending.slice(pending.indexOf(refused) + 1).filter((move) => move.id === refused.id && move.from === refused.to)
}

/** The most characters a section name may have, counted as the host counts them (code points). */
const SECTION_NAME_LIMIT = 40

/** What a name field holds: at most `SECTION_NAME_LIMIT` characters, never cut inside an emoji. */
export function limitSectionName(typed: string): string {
  const chars = [...typed]
  return chars.length > SECTION_NAME_LIMIT ? chars.slice(0, SECTION_NAME_LIMIT).join('') : typed
}
