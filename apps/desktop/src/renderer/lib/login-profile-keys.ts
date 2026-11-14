import type { LoginProfile } from '@shared/models'

/** One variable of the key form: `saved` is one the login profile already has (its value stays on the computer), `typed` one whose name the user types. */
export interface KeyRow {
  id: number
  name: string
  value: string
  saved: boolean
  typed: boolean
}

/** The rows the key form opens with: the login profile's variables, else the names the agent's key usually goes in, else one empty row. */
export function keyRows(profile: Pick<LoginProfile, 'vars'>, loginKeys: string[]): KeyRow[] {
  const saved = profile.vars ?? []
  const names = saved.length ? saved : loginKeys
  if (!names.length) return [newKeyRow(0)]
  return names.map((name, id) => ({ id, name, value: '', saved: saved.length > 0, typed: false }))
}

/** A row whose name is typed, after the ones with ids below `id`. */
export const newKeyRow = (id: number): KeyRow => ({ id, name: '', value: '', saved: false, typed: true })

/** What Save sends: each named variable with a typed value, and the `removed` saved ones as empty values, which remove them. A typed value wins over a removal of the same name. */
export function keyVars(rows: KeyRow[], removed: string[]): Record<string, string> {
  const vars: Record<string, string> = Object.fromEntries(removed.map((name) => [name, '']))
  for (const row of rows) {
    const name = row.name.trim()
    if (name && row.value) vars[name] = row.value
  }
  return vars
}

/** The second line of an API key profile's row, before translation: "API key", then its variable names or "No key yet". */
export const keyDetail = (profile: Pick<LoginProfile, 'vars'>): [string, string] =>
  ['API key', profile.vars?.length ? profile.vars.join(', ') : 'No key yet']
