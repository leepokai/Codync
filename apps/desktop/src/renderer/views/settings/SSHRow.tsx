import { SSH_INSTALL_COMMAND, type SSHProfile, type SSHStatus } from '@shared/ssh'
import { Button, IconButton, Reveal } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, IconSlot, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { analytics } from '../../store/analytics'
import { sshStatusDetail } from '@shared/ssh-startup'

export function SSHRow({ profile, status, edit, attached = true }: { profile: SSHProfile; status: SSHStatus; attached?: boolean; edit: () => void }) {
  const bridge = window.codync.ssh
  const dev = window.codync.appScheme === 'codync-dev'
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
        return dev ? `Install Codync Dev on ${profile.host} and open it once. On Linux, install the development host as ~/.local/bin/codync-dev-host.` : `codync-host isn't installed on ${profile.host}. Install it there:`
    }
  })()
  const problem = status.kind === 'retrying' || status.kind === 'failed' || status.kind === 'notInstalled'
  const active = status.kind === 'connected' || status.kind === 'connecting' || status.kind === 'retrying'
  return (
    <div className="settings-card settings-appear">
      <div className="settings-row">
        <IconSlot width={22} tone="secondary">
          <Icon name="terminal" size={13} />
        </IconSlot>
        <VStack grow className="settings-stack">
          <Text size="callout" weight="medium">{profile.name || profile.host}</Text>
          <Text size="caption" tone={problem ? 'attention' : 'secondary'}>{line}</Text>
        </VStack>
        {active ? (
          <IconButton title="Disconnect" icon="stop.circle" onClick={() => bridge.disconnect(profile.id)} />
        ) : (
          <IconButton title="Connect" icon="arrow.clockwise" onClick={() => bridge.connect(profile.id)} />
        )}
        <IconButton title="Edit" icon="pencil" onClick={edit} />
        <IconButton title="Remove" icon="trash" onClick={() => bridge.remove(profile.id)} />
      </div>
      <Reveal show={status.kind === 'confirmHostKey'}>
        {status.kind === 'confirmHostKey' ? (
          <VStack gap={6}>
            <Text size="caption" tone="secondary">
              First connection to {status.name}. Check that its host key fingerprint matches what the computer's owner sees (
              <code className="settings-mono">ssh-keygen -lf /etc/ssh/ssh_host_ed25519_key.pub</code>).
            </Text>
            {status.fingerprints.map((f) => (
              <Text key={f} size="caption" mono className="selectable">{f}</Text>
            ))}
            <HStack align="stretch" gap={8}>
              <Button onClick={() => bridge.trustHostKey(profile.id)}>Trust and connect</Button>
              <Button kind="secondary" onClick={() => bridge.disconnect(profile.id)}>Cancel</Button>
            </HStack>
          </VStack>
        ) : null}
      </Reveal>
      <Reveal show={status.kind === 'notInstalled' && !dev}>
        <HStack gap={6} className="settings-row">
          <span className="settings-code selectable">{SSH_INSTALL_COMMAND}</span>
          <IconButton title="Copy install command" icon="doc.on.doc" onClick={() => { analytics.track({ event: 'link_opened', link: 'install_command' }); window.codync.app.copy(SSH_INSTALL_COMMAND) }} />
        </HStack>
      </Reveal>
    </div>
  )
}
