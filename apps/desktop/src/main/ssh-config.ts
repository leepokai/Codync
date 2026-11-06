import { globSync, readFileSync } from 'node:fs'
import { homedir } from 'node:os'
import { isAbsolute, join } from 'node:path'
import { ipcMain } from 'electron'
import type { SSHConfigAccess, SSHConfigHost, SSHConfigHosts } from '../shared/ssh'
import { offeredHosts, parseHosts } from '../shared/ssh-config'
import { prefs } from './host-controller'

// Hosts from `~/.ssh/config`, offered in Settings → Computers. The file is read only after the user
// allowed it there; the answer is kept in prefs.

const sshDir = join(homedir(), '.ssh')

/** A file's hosts, then those of the files it includes (relative paths are in ~/.ssh, as for ssh). */
function readHosts(file: string, depth = 0): SSHConfigHost[] {
  let text: string
  try {
    text = readFileSync(file, 'utf8')
  } catch {
    return []
  }
  const { hosts, includes } = parseHosts(text)
  // ssh's own nesting limit.
  if (depth >= 16) return hosts
  const path = (p: string) => (p.startsWith('~/') ? join(homedir(), p.slice(2)) : isAbsolute(p) ? p : join(sshDir, p))
  return [...hosts, ...includes.flatMap((p) => globSync(path(p)).flatMap((f) => readHosts(f, depth + 1)))]
}

function configHosts(): SSHConfigHosts {
  const saved = prefs.get('sshConfigAccess')
  const access: SSHConfigAccess = saved === 'allowed' || saved === 'denied' ? saved : 'ask'
  return { access, hosts: access === 'allowed' ? offeredHosts(readHosts(join(sshDir, 'config'))) : [] }
}

export function registerSSHConfig() {
  ipcMain.handle('ssh:configHosts', () => configHosts())
  ipcMain.handle('ssh:setConfigAccess', (_e, access: unknown) => {
    if (access === 'allowed' || access === 'denied') prefs.set('sshConfigAccess', access)
    return configHosts()
  })
}
