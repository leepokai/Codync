import { SSH_LINGER_COMMAND, type SSHProfile, type SSHStatus } from '@shared/ssh'
import { Button, IconButton, Reveal, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, IconSlot, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { analytics } from '../../store/analytics'
import { sshStatusDetail } from '@shared/ssh-startup'
import { useTr } from '../../lib/use-language'
import { useSSH } from './ssh-model'

export function SSHRow({ profile, status, edit, attached = true }: { profile: SSHProfile; status: SSHStatus; attached?: boolean; edit: () => void }) {
  const tr = useTr()
  const ssh = useSSH()
  const bridge = window.codync.ssh
  // There's no released development host to install.
  const dev = window.codync.appScheme === 'codync-dev'
  const named = (text: string) => tr(text).replaceAll('{name}', profile.host)
  const target = `${profile.user ? `${profile.user}@` : ''}${profile.host}${profile.port !== null ? `:${profile.port}` : ''}`
  const line = (() => {
    switch (status.kind) {
      case 'idle':
        return `${target} · Not connected`
      case 'connecting':
        return status.step
      case 'installing':
        return tr(status.step)
      case 'confirmHostKey':
        return `${target} · Confirm the host key`
      case 'connected':
        return `${target} · ${sshStatusDetail(status, attached)}`
      case 'retrying':
        return `${status.message} Retrying…`
      case 'failed':
        return status.message
      case 'notInstalled':
        if (dev) return named('Install Codync Dev on {name} and open it once. On Linux, install the development host as ~/.local/bin/codync-dev-host.')
        return named(status.failure ? "Couldn't install codync-host on {name}." : "codync-host isn't installed on {name}.")
    }
  })()
  const problem = status.kind === 'retrying' || status.kind === 'failed' || status.kind === 'notInstalled'
  const active = status.kind === 'connected' || status.kind === 'connecting' || status.kind === 'retrying' || status.kind === 'installing'
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
        {status.kind === 'installing' ? <Spinner /> : null}
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
        <HStack gap={8} className="settings-row">
          {status.kind === 'notInstalled' && status.failure?.output ? (
            <span className="settings-code settings-output selectable">{status.failure.output}</span>
          ) : <Spacer />}
          <Button onClick={() => ssh.install(profile.id)}>Install</Button>
        </HStack>
      </Reveal>
      <Reveal show={ssh.state.needsLinger.includes(profile.id)}>
        <VStack gap={6}>
          <Text size="caption" tone="secondary">{named('To keep codync-host running on {name} after you log out, run this there:')}</Text>
          <HStack gap={6}>
            <span className="settings-code selectable">{SSH_LINGER_COMMAND}</span>
            <IconButton title="Copy command" icon="doc.on.doc" onClick={() => { analytics.track({ event: 'link_opened', link: 'install_command' }); window.codync.app.copy(SSH_LINGER_COMMAND) }} />
          </HStack>
        </VStack>
      </Reveal>
    </div>
  )
}
