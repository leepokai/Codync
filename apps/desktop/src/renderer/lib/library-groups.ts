/** Where something falls in a chat's Library (Grok Bot's), by when it was published or sent. */
export type LibraryGroup = 'Today' | 'Yesterday' | 'This week' | 'Earlier'

const DAY = 24 * 60 * 60 * 1000

export function libraryGroup(at: number, now = Date.now()): LibraryGroup {
  const today = new Date(now).setHours(0, 0, 0, 0)
  if (at >= today) return 'Today'
  if (at >= today - DAY) return 'Yesterday'
  if (at >= today - 6 * DAY) return 'This week'
  return 'Earlier'
}

/** Items already newest first, in their groups, in that order. */
export function libraryGroups<T extends { at: number }>(items: T[], now = Date.now()): { group: LibraryGroup; items: T[] }[] {
  const groups: { group: LibraryGroup; items: T[] }[] = []
  for (const item of items) {
    const group = libraryGroup(item.at, now)
    const last = groups[groups.length - 1]
    if (last?.group === group) last.items.push(item)
    else groups.push({ group, items: [item] })
  }
  return groups
}
