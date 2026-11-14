import type { BotDraft } from '@shared/models'

/**
 * Swift's `JSONEncoder` with `.prettyPrinted, .sortedKeys, .withoutEscapingSlashes`; nil fields are left out, and so
 * are fallbacks (they depend on the agents a computer has) and who reaches the bot and the account it runs on (those belong to its computer).
 */
export function encodeTemplate(draft: BotDraft): string {
  const {
    id: _id, pinned: _pinned, hidden: _hidden, fallbacks: _fallbacks, selfFallbacks: _selfFallbacks,
    offeredToNetwork: _offered, reachableByAssistants: _reachable, account: _account, ...rest
  } = draft
  const sorted = Object.fromEntries(
    Object.entries(rest)
      .filter(([, v]) => v !== null && v !== undefined)
      .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
  )
  return JSON.stringify(sorted, null, 2).replace(/^(\s*"(?:[^"\\]|\\.)*"): /gm, '$1 : ')
}
