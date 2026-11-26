import type { Login } from '../../../shared/fallbacks.ts'
import { loginProfileChoices, type Backend } from '../../../shared/models.ts'

/** One entry of the model panel's rail: an agent on one of its logins, named on hover, its login's initials on a badge. */
export interface Seat { key: string; backend: string; name: string; badge?: string; on: boolean; pick: () => void }

/** The initials a login's badge shows. */
export const initials = (name: string) => name.split(/[\s@._-]+/).filter(Boolean).slice(0, 2).map((w) => w[0]).join('').toUpperCase()

/**
 * Every login of `backends` as the rail's seats, `on` the one in use: each agent's default login, then its login
 * profiles. `logins`: the ones of an agent that can be picked (`null` its default login), when not all can; by default
 * (or when it answers none) the default login and the signed-in profiles, with the one in use.
 */
export function loginSeats({ backends, on, pick, tr, logins }: {
  backends: Backend[]
  on: Login
  pick: (backend: Backend, loginProfile: string | null) => void
  tr: (text: string) => string
  logins?: (backend: Backend) => (string | null)[] | undefined
}): Seat[] {
  return backends.flatMap((b) => {
    const current = b.id === on.backend ? on.loginProfile ?? null : undefined
    const ids = logins?.(b) ?? [null, ...loginProfileChoices(b, current ?? null).map((p) => p.id)]
    const agent = b.available || b.id === 'imate' ? b.name : tr(`${b.name} (not installed)`)
    return ids.map((id) => {
      const login = id
        ? b.loginProfiles?.find((p) => p.id === id)?.name ?? tr('New login profile')
        : b.loginProfiles ? b.defaultLoginProfile?.name ?? tr('Default login profile') : null
      return {
        key: `${b.id}/${id ?? ''}`,
        backend: b.id,
        name: login ? `${agent} · ${login}` : agent,
        badge: id && login ? initials(login) : undefined,
        on: current === id,
        pick: () => pick(b, id),
      }
    })
  })
}

/**
 * The seats a chat or a new thread can run on now (`loginSeats`): the installed agents, each on its signed-in logins,
 * with the one in use (`on`). `profiles` false (a host that can't start a thread on a login) keeps each agent to its
 * default login, and `on`'s to its own.
 */
export function readySeats({ backends, on, pick, tr, profiles = true }: Omit<Parameters<typeof loginSeats>[0], 'logins'> & { profiles?: boolean }): Seat[] {
  return loginSeats({
    backends: backends.filter((b) => b.available || b.id === 'imate' || b.id === on.backend), on, pick, tr,
    logins: (b) => {
      const current = b.id === on.backend ? on.loginProfile ?? null : undefined
      if (!profiles) return [current ?? null]
      // A default login waiting on a sign-in is left out, unless it's the one in use.
      return b.signedIn === false && current !== null ? loginProfileChoices(b, current ?? null).map((p) => p.id) : undefined
    },
  })
}
