/** What the older-copy rule reads of an account computer. */
export interface AccountEntry {
  computerId: string
  name: string
  online?: boolean | null
  lastSeenAt?: number | null
  claimedAt?: number | null
}

/**
 * An offline account computer named like one this app uses (`used`) or sees online: an earlier identity of it.
 * Seen after that one joined the account, it ran beside it: another machine with the same name.
 */
export function isOlderCopy(c: AccountEntry, account: AccountEntry[], used: { id: string; name: string }[]) {
  if (c.online) return false
  const replacedBy = (id: string) => {
    const claimedAt = account.find((o) => o.computerId === id)?.claimedAt
    return !claimedAt || !c.lastSeenAt || c.lastSeenAt <= claimedAt
  }
  return used.some((x) => x.id !== c.computerId && x.name === c.name && replacedBy(x.id)) ||
    account.some((o) => o.computerId !== c.computerId && o.name === c.name &&
      (used.some((x) => x.id === o.computerId) || !!o.online) && replacedBy(o.computerId))
}
