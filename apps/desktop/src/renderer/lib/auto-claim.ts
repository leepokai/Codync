/** What the auto-claim rule reads of this computer's host. */
export interface ClaimCandidate {
  id: string
  online: boolean
  cloud: { enabled: boolean; owner?: unknown } | null
}

/**
 * Whether this computer joins the signed-in account on its own. Only with the cloud on: joining needs
 * it, and turning it back on is never done silently. Never an identity the user took out here
 * (`keptOut`), nor one this app has seen in the account (`seen`): without an owner now, it was removed,
 * maybe from a phone, and stays out until it's added by hand. A new identity (a host reset) joins.
 */
export function mayAutoClaim(host: ClaimCandidate, keptOut: readonly string[], seen: readonly string[]) {
  return host.online && !!host.cloud?.enabled && !host.cloud.owner && !keptOut.includes(host.id) && !seen.includes(host.id)
}
