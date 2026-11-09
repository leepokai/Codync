import { createHash } from 'node:crypto'

/** The far side rejected our key (BatchMode never prompts, so a password or passphrase can't help). */
export const authRefused = (stderr: string) => stderr.includes('Permission denied') || stderr.includes('Too many authentication failures')

/** What `ssh -G` resolved the destination to. */
export interface Resolved {
  hostname: string
  port: number
  hostKeyAlias: string | null
  proxyJump: string | null
  proxyCommand: string | null
  /** What the connection will negotiate, so a key read before it (Windows) is one the connection accepts. */
  hostKeyAlgorithms: string | null
  kexAlgorithms: string | null
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
  return {
    hostname, port, hostKeyAlias: set('hostkeyalias'), proxyJump: set('proxyjump'), proxyCommand: set('proxycommand'),
    hostKeyAlgorithms: set('hostkeyalgorithms'), kexAlgorithms: set('kexalgorithms'),
  }
}

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
