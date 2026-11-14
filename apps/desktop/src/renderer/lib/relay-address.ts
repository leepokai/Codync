/** The relay address to store, or null when the typed text already matches the saved one. */
export function relayAddressToSave(typed: string, saved: string | null | undefined): string | null {
  const next = typed.trim().replace(/\/+$/, '')
  const current = (saved ?? '').trim().replace(/\/+$/, '')
  return next === current ? null : next
}
