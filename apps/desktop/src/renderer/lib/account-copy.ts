/** What the interface says when a computer joins or leaves an account (docs/plans/require-sign-in.md, phase 2). */

type Tr = (text: string) => string

/** "123 456" for a 6-digit code. */
export const spacedCode = (code: string) => (code.length === 6 ? `${code.slice(0, 3)} ${code.slice(3)}` : code)

/** Fills `{key}` in one pass: a device named "{code}" stays as written. */
const fill = (text: string, values: Record<string, string>) => text.replace(/\{(\w+)\}/g, (match, key: string) => values[key] ?? match)

/** A signed-in phone scanned a computer in no account: letting it in brings the computer into the phone's account. */
export function pairingAskText(tr: Tr, request: { deviceName: string; email?: string | null; code: string }) {
  // A no-break space: the code never wraps apart.
  const values = { name: request.deviceName, email: request.email || tr('A Codync account'), code: spacedCode(request.code).replace(' ', '\u00a0') }
  return {
    title: fill(tr('Let {name} in?'), values),
    message: fill(tr("{email} wants to use this computer, which then joins {email}'s account. Check {name} shows {code}."), values),
  }
}

/** A computer that changes its account drops every device of the old one, QR-paired ones included. */
export function devicesLoseAccess(tr: Tr, email: string | null | undefined) {
  return email ? fill(tr("Every device of {email}'s account loses access."), { email }) : tr('Every device of that account loses access.')
}
