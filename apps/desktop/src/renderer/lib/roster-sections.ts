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
 * A computer's roster (already in roster order, hidden bots left out) as lines: pinned rows, then
 * each section by name with its rows, then rows without a section, under a "Bots" label when there
 * are sections. A pinned bot shows only among the pinned rows. A folded section still holds its
 * rows, so its fold can animate.
 */
export function rosterLines<T extends { bot: Placed }>(items: T[], folded: readonly string[]): RosterLine<T>[] {
  const row = (item: T): RosterLine<T> => ({ kind: 'row', item })
  const unpinned = items.filter((item) => !item.bot.pinned)
  const sections = sectionNames(unpinned.map((item) => item.bot)).map((name): RosterLine<T> => {
    const rows = unpinned.filter((item) => item.bot.section === name)
    const unread = rows.reduce((sum, item) => sum + item.bot.unread, 0)
    return { kind: 'section', name, count: rows.length, unread, folded: folded.includes(name), items: rows }
  })
  const loose = unpinned.filter((item) => !item.bot.section).map(row)
  const label: RosterLine<T>[] = sections.length && loose.length ? [{ kind: 'label' }] : []
  return [...items.filter((item) => item.bot.pinned).map(row), ...sections, ...label, ...loose]
}

/** The rows on screen, top to bottom: a folded section's rows are left out. */
export function shownRows<T>(lines: RosterLine<T>[]): T[] {
  return lines.flatMap((line) => (line.kind === 'row' ? [line.item] : line.kind === 'label' || line.folded ? [] : line.items))
}

/** Whether a drag (or Alt+↑/↓) may move one bot past another: same pin state and, unpinned, the same section. */
export function sameRosterGroup(a: Omit<Placed, 'unread'>, b: Omit<Placed, 'unread'>): boolean {
  return a.pinned === b.pinned && (a.pinned || (a.section || null) === (b.section || null))
}

/** Every section among `bots` once, by name as the user's language sorts it ("İş" too). */
export function sectionNames(bots: Iterable<{ section?: string }>): string[] {
  const names = new Set<string>()
  for (const bot of bots) if (bot.section) names.add(bot.section)
  return [...names].sort((a, b) => a.localeCompare(b, undefined, { sensitivity: 'base' }))
}

/** `renameSection`'s payload: `to` trimmed, and null (sent, not left out) to remove the section. */
export function sectionRename(from: string, to: string | null): { from: string; to: string | null } {
  return { from, to: to?.trim() || null }
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

/** The bots with these ids still there, each put in `section` (none: undefined) and otherwise as they are now: a section change shown, or a refused one taken back. */
export function placedBots<T extends { section?: string }>(bots: ReadonlyMap<string, T>, ids: readonly string[], section: string | undefined): [string, T][] {
  return ids.flatMap((id): [string, T][] => {
    const bot = bots.get(id)
    return bot ? [[id, { ...bot, section }]] : []
  })
}

/** The most characters a section name may have, counted as the host counts them (code points). */
const SECTION_NAME_LIMIT = 40

/** What a name field holds: at most `SECTION_NAME_LIMIT` characters, never cut inside an emoji. */
export function limitSectionName(typed: string): string {
  const chars = [...typed]
  return chars.length > SECTION_NAME_LIMIT ? chars.slice(0, SECTION_NAME_LIMIT).join('') : typed
}
