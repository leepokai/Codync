import { win32 } from 'node:path'
import type { SSHProfile } from '../shared/ssh.ts'
import type { Resolved } from './ssh-parse.ts'

// Preserve the login shell's PATH, then try the supported installers and the Mac app bundle.
const REMOTE_PATH = '$PATH:/opt/homebrew/bin:/usr/local/bin:/home/linuxbrew/.linuxbrew/bin:$HOME/.local/bin:/Applications/Codync.app/Contents/Resources'

/** The caller validates the port before constructing the remote shell command. */
export const remoteInfoCommand = (port: number, folder = '.codync') => {
  // Only the fixed development folder is accepted; no arbitrary input enters this shell command.
  const dev = folder === '.codync-dev'
  if (dev) {
    return `sh -lc 'export CODYNC_HOME="$HOME/.codync-dev"; if [ -x "/Applications/Codync Dev.app/Contents/Resources/codync-host" ]; then exec "/Applications/Codync Dev.app/Contents/Resources/codync-host" info --json --port ${port}; elif [ -x "$HOME/.local/bin/codync-dev-host" ]; then exec "$HOME/.local/bin/codync-dev-host" info --json --port ${port}; else exit 127; fi'`
  }
  return `sh -lc 'PATH="${REMOTE_PATH}" codync-host info --json --port ${port}'`
}

/** Codync's own known_hosts file in `dir`, this app's host data folder (`~/.codync`, or a development sandbox's). */
export const codyncKnownHosts = (dir: string) => `${dir.replaceAll('\\', '/')}/ssh_known_hosts`

export const knownHostsFiles = (h: string, dir = `${h}/.codync`) => [`${h.replaceAll('\\', '/')}/.ssh/known_hosts`, codyncKnownHosts(dir)]

const escapeRegExp = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')

/** `pkill -f` pattern for the tunnels that use `dir`'s known_hosts file: this copy's own, never another copy's. */
export const orphanTunnelPattern = (binary: string, dir: string) => `^${escapeRegExp(binary)} -N .*${escapeRegExp(codyncKnownHosts(dir))}`

export interface WindowsProcess { ProcessId: number; ExecutablePath: string | null; CommandLine: string | null }

/** Windows' `orphanTunnelPattern`: a native ssh tunnel carrying `dir`'s exact quoted known_hosts file. */
export function ownsWindowsTunnel(candidate: WindowsProcess, binary: string, dir: string) {
  const { ProcessId: pid, ExecutablePath: path, CommandLine: command } = candidate
  if (!path || !command || !Number.isSafeInteger(pid) || pid <= 0) return false
  if (win32.normalize(path).toLowerCase() !== win32.normalize(binary).toLowerCase()) return false
  const marker = codyncKnownHosts(dir)
  return /\s-N\s+-T\s/.test(command) && command.includes('UserKnownHostsFile=') &&
    (command.includes(`\\"${marker}\\"`) || command.includes(`"${marker}"`)) &&
    /\s-L\s+127\.0\.0\.1:\d+:127\.0\.0\.1:\d+(?:\s|$)/.test(command)
}

/** ssh's config tokenizer splits `UserKnownHostsFile` on spaces, so each path carries literal quotes. */
export const hostKeyOptions = (h: string, dir?: string) => [
  '-o', 'UserKnownHostsFile=' + knownHostsFiles(h, dir).map((f) => `"${f}"`).join(' '),
  '-o', 'StrictHostKeyChecking=yes',
]

function targetOptions(p: SSHProfile) {
  const args: string[] = []
  if (p.port !== null) args.push('-p', String(p.port))
  if (p.identityFile !== null) args.push('-i', p.identityFile)
  if (p.user !== null) args.push('-l', p.user)
  return args
}

export const keyscanArguments = (r: Resolved) => ['-T', '5', '-p', String(r.port), '--', r.hostname]

/**
 * Like ssh-keyscan, but ssh itself files the key the host presents in `file` (accept-new) and stops at sign-in: no
 * config and no sign-in method, with the algorithms the connection resolved, so the key filed is the one it will check.
 */
export function presentedKeyArguments(r: Resolved, file: string) {
  const known = `"${file.replaceAll('\\', '/')}"`
  const algorithms = [['HostKeyAlgorithms', r.hostKeyAlgorithms], ['KexAlgorithms', r.kexAlgorithms]]
    .flatMap(([name, value]) => value ? ['-o', `${name}=${value}`] : [])
  return [
    '-F', 'none', '-T', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=5', '-o', 'PreferredAuthentications=none',
    '-o', 'StrictHostKeyChecking=accept-new', '-o', `UserKnownHostsFile=${known}`, '-o', `GlobalKnownHostsFile=${known}`,
    ...algorithms, '-p', String(r.port), '--', r.hostname,
  ]
}

export function resolveArguments(p: SSHProfile) {
  const args = ['-G']
  if (p.port !== null) args.push('-p', String(p.port))
  if (p.user !== null) args.push('-l', p.user)
  return [...args, '--', p.host]
}

/** The remote command is fixed; only the validated port number goes into it. */
export const infoArguments = (p: SSHProfile, h: string, folder = '.codync', dir = `${h}/${folder}`) => [
  '-T', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', '-o', 'ForwardAgent=no', '-o', 'ForwardX11=no',
  ...hostKeyOptions(h, dir), ...targetOptions(p),
  '--', p.host, remoteInfoCommand(p.remotePort, folder),
]

export const tunnelArguments = (p: SSHProfile, localPort: number, h: string, dir?: string) => [
  '-N', '-T', '-o', 'ExitOnForwardFailure=yes', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=3',
  // A shared master owns the listener and lets this child exit. The app must own
  // its tunnel process to verify the listener and close the forward on disconnect.
  '-S', 'none',
  '-o', 'ForwardAgent=no', '-o', 'ForwardX11=no', '-o', 'BatchMode=yes',
  ...hostKeyOptions(h, dir),
  '-L', `127.0.0.1:${localPort}:127.0.0.1:${p.remotePort}`,
  ...targetOptions(p), '--', p.host,
]
