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

const INSTALL_SCRIPT = 'https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh'
/** The service step prints this when linger is off and turning it on needs sudo (`SSH_LINGER_COMMAND`). */
export const LINGER_NEEDED = 'codync-linger-needed'

/** Install's two remote steps, in order: each its own SSH command, so the card can say which one runs. */
export const installSteps = [
  { kind: 'download', title: 'Installing codync-host…', timeout: 300_000 },
  { kind: 'service', title: 'Starting codync-host…', timeout: 120_000 },
] as const
export type InstallStep = typeof installSteps[number]['kind']

/**
 * The released host into `~/.local/bin` (where `remoteInfoCommand` looks), then its service on the profile's
 * validated port. Linux keeps a user's services running after logout only with linger, which usually needs
 * sudo: the service step tries without it and prints `LINGER_NEEDED` when that fails.
 */
export const remoteInstallCommand = (step: InstallStep, port: number) => step === 'download'
  // Read whole first: piped straight in, a failed download would be an empty script that "succeeds".
  ? `sh -lc 'script=$(curl -fsSL ${INSTALL_SCRIPT}) && printf "%s\\n" "$script" | CODYNC_BIN_DIR="$HOME/.local/bin" sh -s -- --host-only'`
  : `sh -lc 'PATH="$HOME/.local/bin:$PATH"; codync-host install --port ${port} && { [ "$(uname -s)" != Linux ] || [ "$(loginctl show-user "$(id -un)" -p Linger --value 2>/dev/null)" = yes ] || loginctl --no-ask-password enable-linger 2>/dev/null || echo ${LINGER_NEEDED}; }'`

/** Whether the service step said linger needs the user (`LINGER_NEEDED`). */
export const needsLinger = (stdout: string) => stdout.split(/\r?\n/).some((line) => line.trim() === LINGER_NEEDED)

/** The last lines an install printed, for the card: its errors (stderr) if any, without colors or progress redraws. */
export function installOutput(stdout: string, stderr: string, count = 6) {
  const lines = (text: string) => text.replace(/\x1b\[[0-9;]*[A-Za-z]/g, '').split(/\r\n|\r|\n/).map((l) => l.trimEnd()).filter(Boolean)
  const err = lines(stderr)
  return (err.length > 0 ? err : lines(stdout)).slice(-count).join('\n')
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

/** One fixed remote command; only the validated port number goes into it. */
const commandArguments = (p: SSHProfile, h: string, dir: string | undefined, command: string) => [
  '-T', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', '-o', 'ForwardAgent=no', '-o', 'ForwardX11=no',
  ...hostKeyOptions(h, dir), ...targetOptions(p),
  '--', p.host, command,
]

export const infoArguments = (p: SSHProfile, h: string, folder = '.codync', dir = `${h}/${folder}`) =>
  commandArguments(p, h, dir, remoteInfoCommand(p.remotePort, folder))

export const installArguments = (p: SSHProfile, step: InstallStep, h: string, dir?: string) =>
  commandArguments(p, h, dir, remoteInstallCommand(step, p.remotePort))

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
