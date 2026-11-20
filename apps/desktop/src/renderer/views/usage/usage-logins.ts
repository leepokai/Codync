import type { UsageProvider, UsageWindow } from '@shared/models'

/** A plan window. Notes (today's tokens, a dollar balance) are not limits. */
export const isLimit = (w: UsageWindow) => !w.text

/** One login of an agent with its windows: the default login has no id. */
export interface UsageLogin {
  id: string | null
  name: string | null
  windows: UsageWindow[]
  /** Since when its checks fail, which leaves `windows` from before. */
  failedAt?: number
}

/** The agent's logins, the default one first. One, unnamed, when it has no login profiles. */
export function logins(p: UsageProvider): UsageLogin[] {
  const profiles = p.loginProfiles ?? []
  const own = { id: null, name: profiles.length ? (p.defaultLoginProfile ?? null) : null, windows: p.windows, failedAt: p.failedAt }
  return [own, ...profiles]
}

/** Shown, but not current: its last check failed. One with nothing read says "No reading yet" instead. */
export const isOutOfDate = (l: UsageLogin) => l.failedAt != null && l.windows.length > 0

const tightestOf = (windows: UsageWindow[]) =>
  windows.filter(isLimit).reduce<UsageWindow | null>((a, w) => (!a || w.percent > a.percent ? w : a), null)

/** The login nearest a limit, which the island shows for the agent: the default one when none has a limit. */
export function tightestLogin(p: UsageProvider): UsageLogin {
  const all = logins(p)
  return all.reduce((a, l) => ((tightestOf(l.windows)?.percent ?? -1) > (tightestOf(a.windows)?.percent ?? -1) ? l : a), all[0])
}

/** The agent's limit nearest its cap, on any of its logins. */
export const tightest = (p: UsageProvider) => tightestOf(tightestLogin(p).windows)
