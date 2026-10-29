/** What the auto-claim rule reads of this computer's host. */
export interface ClaimCandidate {
  id: string
  online: boolean
  cloud: { enabled: boolean; owner?: unknown; relay?: string | null } | null
}

/**
 * Whether this computer joins the signed-in account on its own. Only with the Codync cloud on: joining
 * needs it (a private relay has no accounts), and turning it back on is never done silently. Never an
 * identity the user took out here (`keptOut`), nor one this app has seen in the account (`seen`): without
 * an owner now, it was removed, maybe from a phone, and stays out until it's added by hand. A new
 * identity (a host reset) joins.
 */
export function mayAutoClaim(host: ClaimCandidate, keptOut: readonly string[], seen: readonly string[]) {
  const cloud = host.cloud
  return host.online && !!cloud?.enabled && cloud.relay !== 'private' && !cloud.owner && !keptOut.includes(host.id) && !seen.includes(host.id)
}
