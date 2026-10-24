// SSH computers (spec §11.4): profiles, tunnel status and the computers they attach.
// The main process runs OpenSSH; the renderer shows the state and attaches a store per tunnel.

/** A computer reached over SSH. Nothing secret: keys stay in ssh-agent or on disk. */
export interface SSHProfile {
  id: string
  /** An `~/.ssh/config` alias or a hostname. */
  host: string
  port: number | null
  user: string | null
  identityFile: string | null
  remotePort: number
  /** Remembered after the first connection; a different computer behind the same address is refused. */
  computerId: string | null
  name: string
}

export type SSHStatus =
  | { kind: 'idle' }
  | { kind: 'connecting'; step: string }
  /** First time: the user compares these fingerprints before anything connects. */
  | { kind: 'confirmHostKey'; name: string; fingerprints: string[] }
  | { kind: 'connected'; localPort: number }
  /** A retry is scheduled. */
  | { kind: 'retrying'; message: string }
  /** Needs the user (settings, the remote side, or a security problem); nothing retries. */
  | { kind: 'failed'; message: string }
  | { kind: 'notInstalled' }

/** A connected tunnel: the computer behind it, as a loopback host. */
export interface SSHAttachment {
  profileId: string
  computer: { id: string; name: string; signKey: string; boxKey: string | null }
  baseURL: string
  token: string
}

export interface SSHState {
  profiles: SSHProfile[]
  status: Record<string, SSHStatus>
  attachments: SSHAttachment[]
}

/** A name ssh takes as a destination without it reading as an option: an alias or a hostname. */
export const isSSHHost = (host: string) => !host.startsWith('-') && /^[A-Za-z0-9._-]+$/.test(host)

/** A `Host` of `~/.ssh/config`, offered as an SSH computer to add. */
export interface SSHConfigHost {
  alias: string
  hostname: string | null
  user: string | null
}

/** Whether the user let Codync read `~/.ssh/config` (asked in Settings → Computers). */
export type SSHConfigAccess = 'ask' | 'allowed' | 'denied'

export interface SSHConfigHosts {
  access: SSHConfigAccess
  /** Empty unless `allowed`. */
  hosts: SSHConfigHost[]
}

export const SSH_INSTALL_COMMAND = 'brew install leepokai/codync/codync-host && codync-host install'

export const newSSHProfile = (remotePort = 19222): SSHProfile => ({
  id: crypto.randomUUID(),
  host: '',
  port: null,
  user: null,
  identityFile: null,
  remotePort,
  computerId: null,
  name: '',
})

export interface SSHBridge {
  state(): Promise<SSHState>
  onChange(cb: (s: SSHState) => void): () => void
  /** Adds or replaces the profile; resolves with a problem to show, or null once saved. */
  save(profile: SSHProfile): Promise<string | null>
  remove(id: string): void
  connect(id: string): void
  disconnect(id: string): void
  trustHostKey(id: string): void
  /** The system file dialog; the chosen path, or null. */
  chooseKey(): Promise<string | null>
  /** The hosts in `~/.ssh/config`, read only once the user allowed it. */
  configHosts(): Promise<SSHConfigHosts>
  /** The user's answer; `allowed` reads the config right away. */
  setConfigAccess(access: 'allowed' | 'denied'): Promise<SSHConfigHosts>
}
