import type { HostState } from './ipc.ts'
import type { SSHState, SSHStatus } from './ssh.ts'

export function isSSHConnecting(status: SSHStatus): boolean {
  return status.kind === 'connecting' || status.kind === 'retrying'
}

/** Saved remote computers keep their status and recovery controls reachable. */
export function showsChat(host: HostState['kind'], computers: number, ssh: SSHState): boolean {
  return host === 'running' || computers > 0 || ssh.profiles.length > 0
}

/** Statuses here belong to profiles that have not attached a computer yet. */
export function sshSummary(statuses: SSHStatus[]): string {
  const connecting = statuses.filter(isSSHConnecting).length
  const attention = statuses.filter((s) => s.kind === 'failed' || s.kind === 'notInstalled' || s.kind === 'confirmHostKey').length
  const opening = statuses.filter((s) => s.kind === 'connected').length
  const idle = statuses.filter((s) => s.kind === 'idle').length
  return [connecting && `${connecting} connecting…`, attention && `${attention} ${attention === 1 ? 'needs' : 'need'} attention`, opening && `${opening} opening…`, idle && `${idle} not connected`].filter(Boolean).join(' · ')
}

export function sshStatusDetail(status: SSHStatus, attached = true): string {
  switch (status.kind) {
    case 'idle': return 'Not connected'
    case 'connecting': return status.step
    case 'retrying': return `${status.message} Retrying…`
    case 'confirmHostKey': return 'Confirm host key…'
    case 'connected': return attached ? 'Connected' : 'Connected · opening…'
    case 'failed': return status.message
    case 'notInstalled': return 'Host not installed'
  }
}
