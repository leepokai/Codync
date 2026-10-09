// Phone/host compatibility: each side names the oldest version of the other it still works
// with (docs/reference/compatibility.md). Mirrors host/src/compat.rs.

export type VersionMismatch =
  | { kind: 'updateApp'; minimum: string }
  | { kind: 'updateHost'; version: string; minimum: string }

/** The oldest host this app works with. */
export const MIN_HOST = '2.12.0'

/** `major.minor.patch`, ignoring a leading `v` and any pre-release or build suffix. */
export function parseVersion(version: string): number[] | null {
  let core = version.trim()
  if (core.startsWith('v')) core = core.slice(1)
  core = core.split('-')[0]!.split('+')[0]!
  const parts = core.split('.')
  if (parts.length < 1 || parts.length > 3) return null
  const out: number[] = []
  for (const part of parts) {
    if (!/^[0-9]+$/.test(part)) return null
    out.push(Number(part))
  }
  while (out.length < 3) out.push(0)
  return out
}

/** Whether `version` is below `minimum`; an unreadable version never is. */
export function isBelow(version: string, minimum: string): boolean {
  const v = parseVersion(version)
  const m = parseVersion(minimum)
  if (!v || !m) return false
  for (let i = 0; i < 3; i++) {
    if (v[i]! !== m[i]!) return v[i]! < m[i]!
  }
  return false
}

export function checkVersions(app: string, hostVersion: string, minApp: string | null | undefined): VersionMismatch | null {
  if (minApp && isBelow(app, minApp)) return { kind: 'updateApp', minimum: minApp }
  if (isBelow(hostVersion, MIN_HOST)) return { kind: 'updateHost', version: hostVersion, minimum: MIN_HOST }
  return null
}
