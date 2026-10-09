import type { SSHProfile } from '../shared/ssh.ts'

// Preserve the login shell's PATH, then try the supported installers and Mac app layouts.
// Resources is the Electron bundle; MacOS keeps older native app installs reachable.
const REMOTE_PATH = '$PATH:/opt/homebrew/bin:/usr/local/bin:/home/linuxbrew/.linuxbrew/bin:$HOME/.local/bin:/Applications/Codync.app/Contents/Resources:/Applications/Codync.app/Contents/MacOS'

/** The caller validates the port before constructing the remote shell command. */
export const remoteInfoCommand = (port: number) =>
  `sh -lc 'PATH="${REMOTE_PATH}" codync-host info --json --port ${port}'`

export const knownHostsFiles = (h: string) => [`${h}/.ssh/known_hosts`, `${h}/.codync/ssh_known_hosts`]

/** ssh's config tokenizer splits `UserKnownHostsFile` on spaces, so each path carries literal quotes. */
export const hostKeyOptions = (h: string) => [
  '-o', 'UserKnownHostsFile=' + knownHostsFiles(h).map((f) => `"${f}"`).join(' '),
  '-o', 'StrictHostKeyChecking=yes',
]

function targetOptions(p: SSHProfile) {
  const args: string[] = []
  if (p.port !== null) args.push('-p', String(p.port))
  if (p.identityFile !== null) args.push('-i', p.identityFile)
  if (p.user !== null) args.push('-l', p.user)
  return args
}

export function resolveArguments(p: SSHProfile) {
  const args = ['-G']
  if (p.port !== null) args.push('-p', String(p.port))
  if (p.user !== null) args.push('-l', p.user)
  return [...args, '--', p.host]
}

/** The remote command is fixed; only the validated port number goes into it. */
export const infoArguments = (p: SSHProfile, h: string) => [
  '-T', '-o', 'BatchMode=yes', '-o', 'ConnectTimeout=10', '-o', 'ForwardAgent=no', '-o', 'ForwardX11=no',
  ...hostKeyOptions(h), ...targetOptions(p),
  '--', p.host, remoteInfoCommand(p.remotePort),
]

export const tunnelArguments = (p: SSHProfile, localPort: number, h: string) => [
  '-N', '-T', '-o', 'ExitOnForwardFailure=yes', '-o', 'ServerAliveInterval=15', '-o', 'ServerAliveCountMax=3',
  // A shared master owns the listener and lets this child exit. The app must own
  // its tunnel process to verify the listener and close the forward on disconnect.
  '-S', 'none',
  '-o', 'ForwardAgent=no', '-o', 'ForwardX11=no', '-o', 'BatchMode=yes',
  ...hostKeyOptions(h),
  '-L', `127.0.0.1:${localPort}:127.0.0.1:${p.remotePort}`,
  ...targetOptions(p), '--', p.host,
]
