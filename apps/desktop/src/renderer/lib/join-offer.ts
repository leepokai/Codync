import type { CloudStatus } from '@shared/models'

/** What the account rules read of a computer's host. */
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
 * Whether to ask once to add this computer to the signed-in account (`joinable`). Never an identity this app has seen in
 * the account (`seen`): without an owner now, it was removed, maybe from a phone, and stays out until it's added by
 * hand. A new identity (a host reset) is asked about.
 */
export function mayOfferJoin(host: ClaimCandidate, keptOut: readonly string[], seen: readonly string[]) {
  return joinable(host, keptOut) && !host.cloud?.owner && !seen.includes(host.id)
}

/** Whether to ask to add an SSH computer just installed from here: not one in an account already, nor on a private relay (no accounts). */
export const mayJoinAfterInstall = (cloud: Pick<CloudStatus, 'owner' | 'relay'>) => !cloud.owner && cloud.relay !== 'private'

/**
 * Whether to ask to move this computer into the signed-in account: it's in another one, and signing in
 * here with a different account means it goes with that one now. Saying no keeps it out (`keptOut`).
 */
export function mayOfferMove(host: ClaimCandidate, userId: string, keptOut: readonly string[]) {
  const owner = host.cloud?.owner
  return joinable(host, keptOut) && !!owner && owner.userId !== userId
}

/** The other account this computer is in, for the line in the sidebar (`accountMismatch`). */
export type AccountMismatch = { email: string | null }

/**
 * Whether this computer is in another account than the one signed in here. Null when they match, when it's in none (a
 * choice, not a problem: docs/plans/optional-sign-in.md), or when the host can't say (offline).
 */
export function accountMismatch(host: ClaimCandidate, userId: string): AccountMismatch | null {
  const owner = host.online ? host.cloud?.owner : null
  return owner && owner.userId !== userId ? { email: owner.email ?? null } : null
}
