import { isBelow, type VersionMismatch } from '../../shared/compat.ts'

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

/** The words every client uses for a version mismatch (iOS, the TUI and this app). */
export const updateNeededText = {
  title(m: VersionMismatch, host: string) {
    return m.kind === 'updateApp' ? 'Update this app' : `Update Codync on ${host}`
  },
  detail(m: VersionMismatch, host: string, hostVersion: string | null | undefined, os: string | null | undefined, instructions = true) {
    if (m.kind === 'updateApp') {
      const runs = hostVersion ? `${host} runs Codync ${hostVersion} and` : host
      return `${runs} needs this app to be ${m.minimum} or newer. Your chats are safe on the computer.`
    }
    const needs = `${host} runs Codync ${m.version}; this app needs ${m.minimum} or newer.`
    return instructions ? `${needs} ${updateNeededText.how(os)}` : needs
  },
  /** How to update Codync on a computer this app can't update. */
  how(os: string | null | undefined) {
    return os === 'macos' ? 'Open Codync on that Mac and choose Check for Updates.'
      : os === 'linux' ? 'Run codync-host update there.'
      : 'Update Codync on that computer.'
  },
  /** A computer updating at this app's request. */
  updating(host: string) {
    return `Updating Codync on ${host}. This takes a few minutes.`
  },
  /** Why the last update asked of the computer didn't take. */
  notUpdated(last: HostUpdate, host: string) {
    return last.kind === 'failed' ? last.message : `${host} didn't finish updating. Try again, or update Codync on that computer.`
  },
}

/**
 * The Codync row on a computer's card in Settings > Computers (iOS `HostVersionLine`): the version it
 * runs, and an update when one is due, a newer release or a version too old for this app. `detail` is
 * sentences, each its own phrase to translate; `accessory` is a spinner while the computer updates, or
 * the button that asks it to.
 */
export interface HostVersionLine {
  version: string
  detail: string[]
  warns: boolean
  accessory: { kind: 'updating' } | { kind: 'button'; title: string } | null
}

/** `latest` is the latest release the computer found (null while it doesn't know: then no verdict). */
export function hostVersionLine(version: string, latest: string | null | undefined, mismatch: VersionMismatch | null,
  offer: HostUpdateOffer, host: string, os: string | null | undefined): HostVersionLine {
  const line = (detail: string[], warns = false, accessory: HostVersionLine['accessory'] = null) => ({ version, detail, warns, accessory })
  if (offer.kind === 'updating') return line([updateNeededText.updating(host)], false, { kind: 'updating' })
  const minimum = mismatch?.kind === 'updateHost' ? mismatch.minimum : null
  const available = latest && isBelow(version, latest) ? latest : null
  const due = minimum ? `This app needs ${minimum} or newer.` : available ? `Version ${available} is available.` : null
  if (!due) return line(latest ? ['Up to date'] : [])
  if (offer.kind === 'unavailable') return line([due, updateNeededText.how(os)], minimum !== null)
  if (offer.last) return line([updateNeededText.notUpdated(offer.last, host)], true, { kind: 'button', title: 'Try again' })
  return line([due], minimum !== null, { kind: 'button', title: 'Update' })
}
