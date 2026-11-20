import type { RemoteShell, SSHProfile } from '../shared/ssh.ts'
import { remoteShell } from './ssh-command.ts'

type Probe = (profile: SSHProfile) => Promise<{ status: number; stdout: string }>

/**
 * The shell each profile's computer runs commands in (`cmd` and PowerShell on Windows need their own command
 * lines), probed once and kept until the profile changes or a command in the remembered syntax fails.
 */
export class RemoteShells {
  private shells = new Map<string, RemoteShell>()
  private probe: Probe

  constructor(probe: Probe) {
    this.probe = probe
  }

  get(id: string) {
    return this.shells.get(id)
  }

  forget(id: string) {
    this.shells.delete(id)
  }

  /** A probe that doesn't come back leaves the POSIX command to report why; a cancelled one remembers nothing. */
  async of(profile: SSHProfile, token: { readonly cancelled: boolean }): Promise<RemoteShell> {
    const known = this.shells.get(profile.id)
    if (known) return known
    const probe = await this.probe(profile)
    if (probe.status !== 0) return 'posix'
    const shell = remoteShell(probe.stdout)
    // Edited or removed meanwhile: the answer may be another computer's.
    if (!token.cancelled) this.shells.set(profile.id, shell)
    return shell
  }
}
