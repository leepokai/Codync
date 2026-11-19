import { counted } from './fill.ts'
import type { HostState } from './ipc.ts'
import type { SSHState, SSHStatus } from './ssh.ts'

export function isSSHConnecting(status: SSHStatus): boolean {
  return status.kind === 'connecting' || status.kind === 'retrying' || status.kind === 'installing'
}

/** Saved remote computers keep their status and recovery controls reachable. */
export function showsChat(host: HostState['kind'], computers: number, ssh: SSHState): boolean {
  return host === 'running' || computers > 0 || ssh.profiles.length > 0
}

/** Statuses here belong to profiles that have not attached a computer yet. */
export function sshSummary(statuses: SSHStatus[], tr: (text: string) => string): string {
  const connecting = statuses.filter(isSSHConnecting).length
  const attention = statuses.filter((s) => s.kind === 'failed' || s.kind === 'notInstalled' || s.kind === 'confirmHostKey').length
  const opening = statuses.filter((s) => s.kind === 'connected').length
  const idle = statuses.filter((s) => s.kind === 'idle').length
  return [
    connecting && counted(tr('%d connecting…'), connecting),
    attention && counted(attention === 1 ? tr('%d needs attention') : tr('%d need attention'), attention),
    opening && counted(tr('%d opening…'), opening),
    idle && counted(tr('%d not connected'), idle),
  ].filter(Boolean).join(' · ')
}

export function sshStatusDetail(status: SSHStatus, attached = true, tr: (text: string) => string = (text) => text): string {
  switch (status.kind) {
    case 'idle': return tr('Not connected')
    case 'connecting':
    case 'installing': return tr(status.step)
    case 'retrying': return `${status.message} ${tr('Retrying…')}`
    case 'confirmHostKey': return tr('Confirm host key…')
    case 'connected': return attached ? tr('Connected') : tr('Connected · opening…')
    case 'failed': return status.message
    case 'notInstalled': return tr('Host not installed')
  }
}
