import { isSSHHost, type SSHConfigHost } from './ssh.ts'

/**
 * One `ssh_config` file's `Host` names, with the first `HostName` and `User` of each block (as ssh
 * reads them), and its `Include` patterns. Patterns (`*`, `?`, `!`) and `Match` blocks aren't
 * computers; neither are names ssh would take as an option.
 */
export function parseHosts(text: string): { hosts: SSHConfigHost[]; includes: string[] } {
  const hosts: SSHConfigHost[] = []
  const includes: string[] = []
  let block: SSHConfigHost[] = []
  for (const line of text.split(/\r?\n/)) {
    // `Key value` or `Key=value`; only the first `=` separates.
    const [, key = '', rest = ''] = /^(\w+)\s*=?\s*(.*)$/.exec(line.trim()) ?? []
    const values = rest.split(/\s+/).filter(Boolean)
    switch (key.toLowerCase()) {
      case 'host':
        block = values.filter(isSSHHost).map((alias) => ({ alias, hostname: null, user: null }))
        hosts.push(...block)
        break
      case 'match':
        block = []
        break
      case 'hostname':
        for (const h of block) h.hostname ??= values[0] ?? null
        break
      case 'user':
        for (const h of block) h.user ??= values[0] ?? null
        break
      case 'include':
        includes.push(...values)
        break
    }
  }
  return { hosts, includes }
}

/** Hosts worth offering as computers: each alias once, without git hosting aliases (user `git`). */
export const offeredHosts = (hosts: SSHConfigHost[]) =>
  hosts.filter((h, i) => h.user !== 'git' && hosts.findIndex((o) => o.alias === h.alias) === i)
