import type { BotDraft } from './models'

/**
 * Swift's `JSONEncoder` with `.prettyPrinted, .sortedKeys, .withoutEscapingSlashes`; nil fields are left out. A
 * template carries what the bot is, not this computer's specifics: its account, its team management, or its
 * fallbacks (they depend on the agents a computer has).
 */
export function encodeTemplate(draft: BotDraft): string {
  const {
    id: _id, pinned: _pinned, hidden: _hidden, account: _account, manageTeam: _manage, fallbacks: _fallbacks,
    selfFallbacks: _selfFallbacks, offeredToNetwork: _offered, reachableByAssistants: _reachable, ...rest
  } = draft
  const sorted = Object.fromEntries(
    Object.entries(rest)
      .filter(([, v]) => v !== null && v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  )
  return JSON.stringify(sorted, null, 2).replace(/^(\s*"(?:[^"\\]|\\.)*"): /gm, '$1 : ')
}
