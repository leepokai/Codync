/** What the interface says when a computer joins or leaves an account (docs/plans/optional-sign-in.md). */
import type { PairingRequest } from '@shared/models'
import { fill } from './fill.ts'

type Tr = (text: string) => string

/** "123 456" for a 6-digit code. */
export const spacedCode = (code: string) => (code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code)

/** The phones a join removes (docs/plans/optional-sign-in.md), or nothing when it removes none. */
export function joinRemovesText(tr: Tr, names: readonly string[] | null | undefined) {
  return names?.length ? fill(tr('These phones lose access and must pair again: {names}.'), { names: names.join(', ') }) : ''
}

/** `text`, then the phones a join removes. `null`: the computer couldn't say, so the warning stays general. */
export const withJoinRemoves = (tr: Tr, text: string, names: readonly string[] | null | undefined) =>
  [text, names === null ? tr('Phones paired without this account lose access and must pair again.') : joinRemovesText(tr, names)].filter(Boolean).join(' ')

/**
 * A phone scanned a computer in no account, or a paired phone asks to add it to its account. A signed-in phone may
 * bring the computer along; without that the phone connects only over Wi-Fi or Tailscale.
 */
export function pairingAskText(tr: Tr, request: Pick<PairingRequest, 'deviceName' | 'email' | 'code' | 'canJoin' | 'phoneOnly' | 'removes'>) {
  // A no-break space: the code never wraps apart.
  const values = { name: request.deviceName, email: request.email || tr('A Codync account'), code: spacedCode(request.code).replace(' ', '\u00a0') }
  const check = fill(tr('Check {name} shows {code}.'), values)
  const body = (text: string) => [withJoinRemoves(tr, fill(tr(text), values), request.removes), check].join(' ')
  // A host before 2.41.0 doesn't say: letting the phone in joins its account, whatever the answer asks.
  if (request.canJoin == null) {
    return { title: fill(tr('Let {name} in?'), values), message: body("{email} wants to use this computer, which then joins {email}'s account.") }
  }
  if (!request.canJoin) {
    return { title: fill(tr('Let {name} in?'), values), message: body('It can connect only over Wi-Fi or Tailscale.') }
  }
  const title = fill(tr("Add this computer to {email}'s account?"), values)
  return {
    title,
    message: body(request.phoneOnly ? 'Without it, this phone can connect only over Wi-Fi or Tailscale.' : '{name} wants to add it.'),
  }
}

/** The answers that let the phone in: `join` says whether the computer joins its account. */
export function pairingAnswers(request: Pick<PairingRequest, 'canJoin' | 'phoneOnly'>): { title: string; join?: boolean }[] {
  if (!request.canJoin) return [{ title: 'Let in' }]
  const stayOut = request.phoneOnly ? [{ title: 'Wi-Fi or Tailscale only', join: false }] : []
  return [...stayOut, { title: 'Add to account', join: true }]
}

/** A computer leaving its account drops every device it has: by name when the computer could list them. */
export function leavingRemovesText(tr: Tr, email: string | null | undefined, names: readonly string[] | null | undefined) {
  return joinRemovesText(tr, names) || devicesLoseAccess(tr, email)
}

/** A computer that changes its account drops every device of the old one, QR-paired ones included. */
export function devicesLoseAccess(tr: Tr, email: string | null | undefined) {
  return email ? fill(tr("Every device of {email}'s account loses access."), { email }) : tr('Every device of that account loses access.')
}
