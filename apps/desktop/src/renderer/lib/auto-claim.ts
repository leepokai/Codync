import type { CloudStatus } from '@shared/models'

/** What the auto-claim rule reads of this computer's host. */
export interface ClaimCandidate {
  id: string
  online: boolean
  cloud: Pick<CloudStatus, 'enabled' | 'owner' | 'relay'> | null
}

/**
 * Whether this computer can join the signed-in account here at all. Only with the Codync cloud on: joining
 * needs it (a private relay has no accounts), and turning it back on is never done silently. Never an
 * identity the user took out here (`keptOut`).
 */
function joinable(host: ClaimCandidate, keptOut: readonly string[]) {
  return host.online && !!host.cloud?.enabled && host.cloud.relay !== 'private' && !keptOut.includes(host.id)
}

/**
 * Whether this computer joins the signed-in account on its own (`joinable`). Never an identity this app
 * has seen in the account (`seen`): without an owner now, it was removed, maybe from a phone, and stays
 * out until it's added by hand. A new identity (a host reset) joins.
 */
export function mayAutoClaim(host: ClaimCandidate, keptOut: readonly string[], seen: readonly string[]) {
  return joinable(host, keptOut) && !host.cloud?.owner && !seen.includes(host.id)
}

/**
 * Whether to ask to move this computer into the signed-in account: it's in another one, and signing in
 * here with a different account means it goes with that one now. Saying no keeps it out (`keptOut`).
 */
export function mayOfferMove(host: ClaimCandidate, userId: string, keptOut: readonly string[]) {
  const owner = host.cloud?.owner
  return joinable(host, keptOut) && !!owner && owner.userId !== userId
}
