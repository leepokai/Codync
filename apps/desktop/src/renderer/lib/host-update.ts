/** An update this app asked of a computer that's behind it (`installHostUpdate`). */
export type HostUpdate = { kind: 'requested'; at: number } | { kind: 'failed'; message: string }

/**
 * What the update notice offers for a computer that's behind (iOS `HostUpdateOffer`): `updating`
 * while a request runs; `available` when the computer updates when asked, with `last` a request
 * that didn't (it failed, or the computer never came back updated); `unavailable` when the notice
 * says how to update it on the computer instead.
 */
export type HostUpdateOffer = { kind: 'updating' } | { kind: 'available'; last: HostUpdate | null } | { kind: 'unavailable' }

/** How long a requested update may take before the notice offers it again. */
export const HOST_UPDATE_WAIT = 10 * 60_000

export function offerHostUpdate(update: HostUpdate | null, canUpdate: boolean, now = Date.now()): HostUpdateOffer {
  if (update?.kind === 'requested' && now - update.at < HOST_UPDATE_WAIT) return { kind: 'updating' }
  return canUpdate ? { kind: 'available', last: update } : { kind: 'unavailable' }
}
