import { createHash } from 'node:crypto'
import { statSync } from 'node:fs'
import { posix, win32 } from 'node:path'
import { isSSHHost, type SSHProfile } from '../shared/ssh.ts'

/** Only native absolute paths: a drive-relative Windows key is ambiguous. */
export const isAbsoluteKeyPath = (path: string, os = process.platform) =>
  os === 'win32' ? /^(?:[A-Za-z]:[\\/]|\\\\[^\\]+\\[^\\]+)/.test(path) && win32.isAbsolute(path) : posix.isAbsolute(path)

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

/** The far side rejected our key (BatchMode never prompts, so a password or passphrase can't help). */
export const authRefused = (stderr: string) => stderr.includes('Permission denied') || stderr.includes('Too many authentication failures')

/** What `ssh -G` resolved the destination to. */
export interface Resolved {
  hostname: string
  port: number
  hostKeyAlias: string | null
  proxyJump: string | null
  proxyCommand: string | null
}

export const usesProxy = (r: Resolved) => r.proxyJump !== null || r.proxyCommand !== null
/** The name ssh looks up in known_hosts. */
export const knownHostsName = (r: Resolved) => {
  const name = r.hostKeyAlias ?? r.hostname
  return r.port === 22 ? name : `[${name}]:${r.port}`
}

export function parseConfig(text: string): Resolved | null {
  const values = new Map<string, string>()
  for (const line of text.split(/\r?\n/)) {
    const at = line.indexOf(' ')
    if (at < 0) continue
    const key = line.slice(0, at).toLowerCase()
    if (!values.has(key)) values.set(key, line.slice(at + 1).trim())
  }
  const set = (k: string) => {
    const v = values.get(k)
    return v && v !== 'none' ? v : null
  }
  const hostname = set('hostname')
  const port = Number(values.get('port'))
  if (!hostname || !Number.isInteger(port) || !values.get('port')) return null
  return { hostname, port, hostKeyAlias: set('hostkeyalias'), proxyJump: set('proxyjump'), proxyCommand: set('proxycommand') }
}

export const keyscanArguments = (r: Resolved) => ['-T', '5', '-p', String(r.port), '--', r.hostname]

/** `type key` of each host key line (known_hosts or ssh-keyscan output); comments and revoked keys skipped. */
export function hostKeys(text: string, certAuthorities = true): Set<string> {
  const keys = new Set<string>()
  for (const line of text.split(/\r?\n/)) {
    const fields = line.split(/\s+/).filter(Boolean)
    const first = fields[0]
    if (!first || first.startsWith('#')) continue
    if (first.startsWith('@')) {
      if (!certAuthorities || first !== '@cert-authority') continue
      fields.shift()
    }
    if (fields.length >= 3) keys.add(`${fields[1]} ${fields[2]}`)
  }
  return keys
}

/** ssh-keyscan's lines, filed under the name ssh will look up. */
export const knownHostsLines = (scan: string, name: string) => [...hostKeys(scan)].sort().map((k) => `${name} ${k}`)

/** A computer id is the first 16 bytes of SHA-256 of its signing key, base64url. */
export function consistent(id: string, signKey: string) {
  const key = Buffer.from(signKey, 'base64url')
  return key.length === 32 && createHash('sha256').update(key).digest().subarray(0, 16).toString('base64url') === id
}

/** What `codync-host info --json` prints. */
interface RemoteInfo {
  name: string
  computerId: string
  signKey: string
  boxKey?: string | null
  token: string
  running: boolean
}

export function parseInfo(text: string): RemoteInfo | null {
  try {
    const v = JSON.parse(text) as Partial<RemoteInfo>
    return typeof v.name === 'string' && typeof v.computerId === 'string' && typeof v.signKey === 'string' && typeof v.token === 'string' && typeof v.running === 'boolean'
      ? (v as RemoteInfo)
      : null
  } catch {
    return null
  }
}
