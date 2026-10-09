import { SSH_INSTALL_COMMAND, type SSHProfile, type SSHStatus } from '@shared/ssh'
import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { font } from '../../lib/fonts'
import { Reveal } from './parts'
import { sshStatusDetail } from '@shared/ssh-startup'
import { sshBridge } from './ssh-model'

export function SSHRow({ profile, status, edit, attached = true }: { profile: SSHProfile; status: SSHStatus; attached?: boolean; edit: () => void }) {
  const bridge = sshBridge()
  const target = `${profile.user ? `${profile.user}@` : ''}${profile.host}${profile.port !== null ? `:${profile.port}` : ''}`
  const line = (() => {
    switch (status.kind) {
      case 'idle':
        return `${target} · Not connected`
      case 'connecting':
        return status.step
      case 'confirmHostKey':
        return `${target} · Confirm the host key`
      case 'connected':
        return `${target} · ${sshStatusDetail(status, attached)}`
      case 'retrying':
        return `${status.message} Retrying…`
      case 'failed':
        return status.message
      case 'notInstalled':
        return `codync-host isn't installed on ${profile.host}. Install it there:`
    }
  })()
  const problem = status.kind === 'retrying' || status.kind === 'failed' || status.kind === 'notInstalled'
  const active = status.kind === 'connected' || status.kind === 'connecting' || status.kind === 'retrying'
  return (
    <div className="settings-card settings-appear">
      <div className="settings-row">
        <span style={{ width: 22, display: 'flex', justifyContent: 'center', color: 'var(--secondary)' }}>
          <Icon name="terminal" size={13} />
        </span>
        <div className="settings-stack" style={{ flex: 1 }}>
          <span style={font('callout', 'medium')}>{profile.name || profile.host}</span>
          <span style={{ ...font('caption'), color: problem ? 'var(--warning)' : 'var(--secondary)' }}>{line}</span>
        </div>
        {active ? (
          <IconButton title="Disconnect" icon="stop.circle" onClick={() => bridge?.disconnect(profile.id)} />
        ) : (
          <IconButton title="Connect" icon="arrow.clockwise" onClick={() => bridge?.connect(profile.id)} />
        )}
        <IconButton title="Edit" icon="pencil" onClick={edit} />
        <IconButton title="Remove" icon="trash" onClick={() => bridge?.remove(profile.id)} />
      </div>
      <Reveal show={status.kind === 'confirmHostKey'}>
        {status.kind === 'confirmHostKey' ? (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 6 }}>
            <span style={{ ...font('caption'), color: 'var(--secondary)' }}>
              First connection to {status.name}. Check that its host key fingerprint matches what the computer's owner sees (
              <code style={{ fontFamily: 'var(--mono)' }}>ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub</code>).
            </span>
            {status.fingerprints.map((f) => (
              <span key={f} className="selectable" style={font('caption', undefined, 'monospaced')}>{f}</span>
            ))}
            <div style={{ display: 'flex', gap: 8 }}>
              <Button onClick={() => bridge?.trustHostKey(profile.id)}>Trust and connect</Button>
              <Button kind="secondary" onClick={() => bridge?.disconnect(profile.id)}>Cancel</Button>
            </div>
          </div>
        ) : null}
      </Reveal>
      <Reveal show={status.kind === 'notInstalled'}>
        <div className="settings-row" style={{ gap: 6 }}>
          <span className="settings-code selectable">{SSH_INSTALL_COMMAND}</span>
          <IconButton title="Copy install command" icon="doc.on.doc" onClick={() => window.codync.app.copy(SSH_INSTALL_COMMAND)} />
        </div>
      </Reveal>
    </div>
  )
}
