// `/skill` in the composer: while the draft is just `/partial`, the bot's skills are offered.
// The host turns a message starting with `/<skill id>` into "read this skill first"
// (host/src/market/skills.rs `expand_command`).

/** The first host with `/skill` messages and skill details; older ones get the text as typed. */
export const SKILLS_HOST = '2.14.1'

/** The skill name being typed after a leading `/`, before any space; otherwise null. */
export function skillQuery(draft: string): string | null {
  return /^\/\S*$/.test(draft) ? draft.slice(1) : null
}

/** The bot's skills (ids in `enabled`) whose id or name holds `query`. */
export function matchingSkills<T extends { id: string; name: string }>(installed: T[], enabled: string[], query: string): T[] {
  const q = query.toLowerCase()
  return installed.filter((s) => enabled.includes(s.id) && (s.id.includes(q) || s.name.toLowerCase().includes(q)))
}
