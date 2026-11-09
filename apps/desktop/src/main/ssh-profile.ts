import { statSync } from 'node:fs'
import { posix } from 'node:path'
import { isSSHHost, type SSHProfile } from '../shared/ssh.ts'

/** Only native absolute paths: a drive-relative Windows key is ambiguous. */
export const isAbsoluteKeyPath = (path: string, os = process.platform) =>
  os === 'win32' ? /^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)/.test(path) : posix.isAbsolute(path)

/** A reason the profile can't be used, or null. */
export function validate(p: SSHProfile): string | null {
  if (!isSSHHost(p.host)) {
    return 'Use an SSH alias or a hostname: letters, digits, dots, dashes and underscores.'
  }
  if (p.user !== null && !/^[A-Za-z_][A-Za-z0-9._-]*$/.test(p.user)) return "That user name isn't valid."
  const inRange = (n: number) => Number.isInteger(n) && n >= 1 && n <= 65535
  if (p.port !== null && !inRange(p.port)) return 'The SSH port must be between 1 and 65535.'
  if (!inRange(p.remotePort)) return 'The Codync port must be between 1 and 65535.'
  if (p.identityFile !== null) {
    let ok = false
    try {
      ok = isAbsoluteKeyPath(p.identityFile) && statSync(p.identityFile).isFile()
    } catch {}
    if (!ok) return "The key file doesn't exist."
  }
  return null
}
