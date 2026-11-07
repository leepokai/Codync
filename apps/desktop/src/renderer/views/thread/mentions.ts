// The composer's `@` list in a group: who it offers, and what picking one writes. The rules for who
// a message reaches are the host's: docs/features/groups-and-replies.md.
import type { Bot } from '../../../shared/models.ts'

/**
 * The `@partial` being typed at the end of the draft, lowercased, if any. A member's full name and
 * its space end it, so the next Return sends instead of offering a longer name that holds this one.
 */
function mentionQuery(draft: string, members: Bot[]): string | null {
  const at = draft.lastIndexOf('@')
  if (at < 0 || (at > 0 && !/\s/.test(draft[at - 1]!))) return null
  const query = draft.slice(at + 1).toLowerCase()
  if (/^\s|\n/.test(query) || query.length > 24) return null
  return members.some((m) => query === `${m.name.toLowerCase()} `) ? null : query
}

/** The group's `members` whose name holds the `@partial` at the end of the draft. */
export function mentionSuggestions(draft: string, members: Bot[]): Bot[] {
  const query = mentionQuery(draft, members)
  return query === null ? [] : members.filter((m) => m.name.toLowerCase().includes(query))
}

/** The draft with the `@partial` at its end replaced by `member`'s name, ready for the message. */
export function withMention(draft: string, member: Bot): string {
  return `${draft.slice(0, draft.lastIndexOf('@'))}@${member.name} `
}

/**
 * What Return does with a member highlighted: pick it, except on a whole `@all` or `@everyone`,
 * which the list doesn't offer and which sends to the room.
 */
export function returnMentions(_picked: Bot, draft: string): boolean {
  return !/(^|\s)@(all|everyone)\s*$/i.test(draft)
}
