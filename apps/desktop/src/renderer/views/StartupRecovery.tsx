import { isSSHConnecting } from '@shared/ssh-startup'
import { Button, Spinner } from '../components/Controls'
import { Icon } from '../components/Icon'
import { font } from '../lib/fonts'
import { useApp } from '../store/context'
import { HostStateView } from './HostStateView'
import { SSHRow } from './settings/SSHRow'
import { useSSH } from './settings/ssh-model'
import './startup-recovery.css'

/** Saved remote computers remain visible while no computer is ready for a chat. */
export function StartupRecovery({ manage }: { manage: () => void }) {
  const app = useApp()
  const ssh = useSSH()
  const connecting = ssh.state.profiles.some((p) => isSSHConnecting(ssh.status(p.id)))
  return (
    <div className="startup-recovery">
      <div className="startup-recovery-content">
        <div className="startup-recovery-title" role="status">
          {connecting ? <Spinner size={22} /> : <Icon name="desktopcomputer" size={22} color="var(--secondary)" />}
          <span style={font('title2', 'semibold')}>{connecting ? 'Connecting to your computers…' : 'Your remote computers'}</span>
        </div>
        <div style={{ color: 'var(--secondary)' }}>Your bots will appear once a computer is connected.</div>
        <div className="startup-recovery-profiles">
          {ssh.state.profiles.map((p) => (
            <SSHRow key={p.id} profile={p} status={ssh.status(p.id)} attached={ssh.state.attachments.some((a) => a.profileId === p.id)} edit={manage} />
          ))}
        </div>
        <Button kind="secondary" onClick={manage}>Manage computers</Button>
        {app.host.state.kind !== 'running' ? <HostStateView compact /> : null}
      </div>
    </div>
  )
}
