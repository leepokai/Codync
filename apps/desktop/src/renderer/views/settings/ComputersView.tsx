import { useCallback, useState } from 'react'
import type { AccountApproval, PairingInfo } from '@shared/models'
import { newSSHProfile, type SSHProfile } from '@shared/ssh'
import { Button, CardSection, IconButton, Reveal, Switch } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, ModalHeader, Sheet, useDismiss } from '../../components/Overlay'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useAccount } from '../../store/account'
import type { CloudComputer } from '../../store/cloud'
import type { BotStore } from '../../store/bot-store'
import { StoreContext, useApp } from '../../store/context'
import { ComputerBadge } from '../ComputerBadge'
import { PairingCode } from '../PairingCode'
import { AuthorizedDevicesSection } from './AuthorizedDevicesSection'
import { useTr } from '../../lib/use-language'
import { errorText, liveClient, relativeDay, SectionHeader, statusText, thisMac } from './parts'
import { useSSH } from './ssh-model'
import { SSHRow } from './SSHRow'
import { VoiceChatSettingsView } from './VoiceChatSettingsView'

/**
 * Computers this app manages (itself and SSH), with their account, relay and authorized devices;
 * SSH profiles; and the account's other computers.
 */
export function ComputersView() {
  const tr = useTr()
  const app = useApp()
  const account = useAccount()
  const sshState = useSSH()
  const cloud = useModel(app.cloud)
  const [editing, setEditing] = useState<{ profile: SSHProfile; isNew: boolean } | null>(null)
  const [removingCopy, setRemovingCopy] = useState<CloudComputer | null>(null)
  const managed = app.managedStores
  const managedIds = new Set(managed.map((s) => s.computer.id))
  const others = cloud.computers.filter((c) => !managedIds.has(c.computerId))

  return (
    <div className="settings-computers">
      <VStack gap={26} className="settings-computers-content">
        {managed.map((store) => (
          <ManagedComputerCard key={store.computer.id} store={store} ssh={sshState.isSSH(store.computer.id)} />
        ))}

        <div>
          <SectionHeader title="Over SSH">
            <IconButton title="Add SSH computer" icon="plus" onClick={() => setEditing({ profile: newSSHProfile(window.codync.hostPort), isNew: true })} />
          </SectionHeader>
          <Reveal show={sshState.state.profiles.length === 0}>
            <Text as="div" size="callout" tone="secondary">
              {tr('Run bots on another computer you reach with SSH. It needs codync-host installed; your SSH keys stay on {name}.').replaceAll('{name}', tr(thisMac))}
            </Text>
          </Reveal>
          {sshState.state.profiles.map((profile) => (
            <SSHRow key={profile.id} profile={profile} status={sshState.status(profile.id)} edit={() => setEditing({ profile, isNew: false })} />
          ))}
        </div>

        <div>
          <SectionHeader title="Other computers in your account">
            {account.isSignedIn ? <IconButton title="Refresh" icon="arrow.clockwise" onClick={() => void cloud.refresh()} /> : null}
          </SectionHeader>
          {!account.isSignedIn ? (
            <Text as="div" size="callout" tone="secondary">{tr(`Sign in to see the other computers in your account.`)}</Text>
          ) : others.length === 0 ? (
            <Text as="div" size="callout" tone="secondary">{tr(`No other computers in your account yet.`)}</Text>
          ) : null}
          {others.map((computer) => (
            <AccountComputerRow key={computer.computerId} computer={computer} olderCopy={cloud.isOlderCopy(computer)} onRemove={() => setRemovingCopy(computer)} />
          ))}
        </div>
      </VStack>

      <Sheet open={editing !== null} onClose={() => setEditing(null)} width={460}>
        {editing ? <SSHProfileEditor key={editing.profile.id} profile={editing.profile} isNew={editing.isNew} /> : null}
      </Sheet>
      <Dialog
        open={removingCopy !== null}
        title={tr('Remove the older {name}?').replaceAll('{name}', removingCopy?.name ?? 'computer')}
        message={tr("It's an earlier identity of {name} and can't be reached anymore. Removing it only cleans up your account.").replaceAll('{name}', removingCopy?.name ?? 'a computer')}
        actions={removingCopy ? [{ title: 'Remove', destructive: true, action: () => void cloud.removeFromAccount(removingCopy.computerId) }] : []}
        onClose={() => setRemovingCopy(null)}
      />
    </div>
  )
}

const isMacLabel = thisMac === 'this Mac' ? 'This Mac' : 'This computer'

/** This computer or an SSH computer: relay, account membership, pairing and authorized devices. */
function ManagedComputerCard({ store, ssh: overSSH }: { store: BotStore; ssh: boolean }) {
  const tr = useTr()
  const app = useApp()
  const account = useAccount()
  useModel(store)
  const [busy, setBusy] = useState(false)
  const [showPairing, setShowPairing] = useState(false)
  const [showVoice, setShowVoice] = useState(false)
  const [confirmUnclaim, setConfirmUnclaim] = useState(false)
  const [confirmAutoApproval, setConfirmAutoApproval] = useState(false)
  const cloud = store.cloud
  const owner = cloud?.owner ?? null
  const autoApproval = cloud?.approval === 'auto'
  const online = store.connection.kind === 'online'
  // A host that needs an update doesn't give its cloud status: nothing there to show or change.
  const manageable = online && !store.mismatch
  const name = store.hostName

  const run = (work: () => Promise<void>) => {
    setBusy(true)
    void work().finally(() => setBusy(false))
  }

  const setApproval = (approval: AccountApproval) =>
    run(async () => {
      try {
        await (await store.ready()).call('setApproval', { approval })
      } catch (e) {
        store.setError(errorText(e))
      }
    })

  const cloudLine = !cloud?.enabled
    ? tr('Off: only Wi-Fi and Tailscale reach it, nothing goes through Cloudflare.')
    : (cloud.lastError ?? tr(cloud.connected ? 'Reachable from anywhere through Cloudflare (encrypted).' : 'Connecting to Cloudflare…'))

  const accountLine = (
    <HStack key="account" gap={8} className="settings-row settings-appear">
      <Icon name={owner ? 'person.crop.circle.badge.checkmark' : 'person.crop.circle.badge.plus'} size={13} color="var(--secondary)" />
      <Text size="callout" grow>
        {owner
          ? owner.userId === account.user?.userId
            ? tr(`In your account`)
            : tr('In {name} account').replaceAll('{name}', owner.email ?? tr('another'))
          : account.isSignedIn
            ? tr(`Not in your account`)
            : tr(`Sign in to add it to your account`)}
      </Text>
      {owner ? (
        <Button kind="secondary" onClick={() => setConfirmUnclaim(true)} disabled={busy || !manageable}>
          Remove from account
        </Button>
      ) : account.isSignedIn ? (
        <Button kind="secondary" onClick={() => run(async () => void (await app.cloud.claim(store)))} disabled={busy || !manageable}>
          {busy ? 'Adding…' : 'Add to account'}
        </Button>
      ) : null}
    </HStack>
  )

  return (
    <VStack gap={28}>
      <CardSection>
        {[
          <div key="header" className="settings-row">
            <ComputerBadge computer={store.computer} size={26} />
            <VStack grow className="settings-stack">
              <Text size="headline">{name}</Text>
              <Text size="caption" tone="secondary">
                {`${tr(overSSH ? 'Over SSH' : isMacLabel)} · ${tr(statusText(store))}`}
              </Text>
            </VStack>
            {overSSH ? (
              // This computer's voice settings are their own Settings page.
              <IconButton title="Voice chat" icon="waveform" selected={showVoice} disabled={!online} onClick={() => setShowVoice((s) => !s)} />
            ) : null}
            <IconButton title="Pair iPhone" icon="qrcode" selected={showPairing} disabled={!online} onClick={() => setShowPairing((s) => !s)} />
          </div>,
          // Until the host gives its cloud status, there's no way to tell its relay or account.
          ...(cloud
            ? [
                <Switch key="cloud" className="settings-appear" on={cloud.enabled} disabled={busy || !manageable} onChange={(on) => run(() => app.cloud.setCloud(store, on))}>
                  <span className="settings-stack">
                    <Text>{tr(`Reach from anywhere`)}</Text>
                    <Text size="caption" tone={cloud.lastError ? 'attention' : 'secondary'}>{cloudLine}</Text>
                  </span>
                </Switch>,
                accountLine,
              ]
            : []),
          ...(owner
            ? [
                <Switch
                  key="approval"
                  on={autoApproval}
                  disabled={busy || !manageable}
                  onChange={(on) => (on ? setConfirmAutoApproval(true) : setApproval('code'))}
                >
                  <span className="settings-stack settings-appear">
                    <Text>{tr(`Skip the 6-digit check`)}</Text>
                    <Text size="caption" tone={autoApproval ? 'attention' : 'secondary'}>
                      {autoApproval ? tr(`Devices on your account get in on their own.`) : tr(`Each new device shows a code you approve here.`)}
                    </Text>
                  </span>
                </Switch>,
              ]
            : []),
        ]}
      </CardSection>

      <AuthorizedDevicesSection store={store} />

      <Sheet open={showVoice} onClose={() => setShowVoice(false)}>
        <StoreContext.Provider value={store}>
          <VoiceChatSettingsView />
        </StoreContext.Provider>
      </Sheet>
      <Sheet open={showPairing} onClose={() => setShowPairing(false)} width={340}>
        <ModalHeader icon="qrcode" title={tr('Pair iPhone with {name}').replaceAll('{name}', name)} />
        <PairingPanel store={store} />
      </Sheet>
      <Dialog
        open={confirmAutoApproval}
        title={tr('Let account devices in without a code?')}
        message={tr("Any device signed in to your account gets full control of {name}: its files, terminals and bots, with no check here. If someone gets into your account, or Codync's cloud is ever compromised, they could add their own device and you'd have no chance to stop it. Devices paired with a QR code aren't affected.").replaceAll('{name}', name)}
        actions={[{ title: 'Skip the check', destructive: true, action: () => setApproval('auto') }]}
        onClose={() => setConfirmAutoApproval(false)}
      />
      <Dialog
        open={confirmUnclaim}
        title={tr('Remove {name} from the account?').replaceAll('{name}', name)}
        message={tr('Devices that were approved through the account lose access. Devices paired with a QR code keep it.')}
        actions={[{ title: 'Remove from account', destructive: true, action: () => run(() => app.cloud.unclaim(store)) }]}
        onClose={() => setConfirmUnclaim(false)}
      />
    </VStack>
  )
}

/** An account computer this app isn't authorized on, or one it uses under an earlier identity (`olderCopy`). */
function AccountComputerRow({ computer, olderCopy, onRemove }: { computer: CloudComputer; olderCopy: boolean; onRemove: () => void }) {
  const tr = useTr()
  return (
    <div className="settings-card settings-appear">
      <div className="settings-row">
        <span className={olderCopy ? 'settings-off' : undefined}>
          <ComputerBadge computer={{ id: computer.computerId, name: computer.name, signKey: computer.signKey, urls: [], device: computer.device ?? null }} size={22} />
        </span>
        <div className="settings-stack">
          <Text size="callout" weight="medium" tone={olderCopy ? 'secondary' : undefined}>{computer.name}</Text>
          <Text size="caption" tone={olderCopy ? 'tertiary' : 'secondary'}>{tr(accountComputerStatus(computer, olderCopy))}</Text>
        </div>
        <Spacer />
        {olderCopy ? <IconButton title="Remove from account" icon="trash" onClick={onRemove} /> : null}
      </div>
    </div>
  )
}

/** "Older copy", not the phones' "Older copy of this computer": on the desktop "this computer" reads as this Mac. */
function accountComputerStatus(computer: CloudComputer, olderCopy: boolean) {
  if (!olderCopy) return computer.online ? 'Online' : 'Offline'
  return computer.lastSeenAt ? `Older copy · last seen ${relativeDay(new Date(computer.lastSeenAt))}` : 'Older copy'
}

/** "Pair iPhone" for a computer this app manages, through its store. */
function PairingPanel({ store }: { store: BotStore }) {
  useModel(store)
  const requestCode = useCallback(async () => {
    const client = liveClient(store)
    if (!client) throw new Error(`${store.hostName} isn't connected.`)
    return client.call<PairingInfo>('pairing')
  }, [store])
  return <PairingCode name={store.hostName} cloudOn={store.cloud?.enabled === true} requestCode={requestCode} />
}

// MARK: SSH

/** Add or change an SSH computer. Only fields ssh takes as separate arguments; nothing goes through a shell. */
function SSHProfileEditor({ profile: initial, isNew }: { profile: SSHProfile; isNew: boolean }) {
  const tr = useTr()
  const dismiss = useDismiss()
  const [profile, setProfile] = useState(initial)
  const [port, setPort] = useState(initial.port?.toString() ?? '')
  const [remotePort, setRemotePort] = useState(String(initial.remotePort))
  const [user, setUser] = useState(initial.user ?? '')
  const [problem, setProblem] = useState<string | null>(null)
  const bridge = window.codync.ssh

  const save = async () => {
    const isInt = (s: string) => /^-?\d+$/.test(s)
    const portText = port.trim()
    if (portText && !isInt(portText)) return setProblem('The SSH port must be a number.')
    const remote = remotePort.trim()
    if (!isInt(remote)) return setProblem('The Codync port must be a number.')
    const u = user.trim()
    const p: SSHProfile = {
      ...profile,
      host: profile.host.trim(),
      name: profile.name.trim(),
      user: u || null,
      port: portText ? Number(portText) : null,
      remotePort: Number(remote),
    }
    const found = await bridge.save(p)
    if (found) return setProblem(found)
    dismiss()
  }

  const field = (label: string, value: string, set: (v: string) => void, prompt: string) => (
    <HStack gap={12} className="settings-row">
      <Text tone="text">{label}</Text>
      <input className="settings-field" aria-label={label} placeholder={prompt} value={value} spellCheck={false} onChange={(e) => set(e.target.value)} />
    </HStack>
  )

  return (
    // Return in a field saves (the default action).
    <VStack className="settings-ssh-editor" onKeyDown={(e) => e.key === 'Enter' && e.target instanceof HTMLInputElement && void save()}>
      <ModalHeader icon="terminal" title={isNew ? 'Add SSH computer' : 'Edit SSH computer'} trailing={<IconButton title={isNew ? 'Add' : 'Save'} icon="checkmark" onClick={() => void save()} />} />
      <VStack gap={16} className="settings-ssh-editor-body">
        <CardSection>
          {field('Name', profile.name, (name) => setProfile({ ...profile, name }), 'Optional')}
          {field('Host', profile.host, (host) => setProfile({ ...profile, host }), 'SSH alias or hostname')}
          {field('User', user, setUser, 'From SSH config')}
          {field('SSH port', port, setPort, 'From SSH config')}
          <div className="settings-row">
            <span>Key</span>
            <Spacer />
            <Text tone="secondary">{profile.identityFile?.split('/').pop() ?? tr(`ssh-agent / SSH config`)}</Text>
            <IconButton
              title="Choose key file"
              icon="key"
              onClick={() => void bridge.chooseKey().then((path) => path && setProfile((p) => ({ ...p, identityFile: path })))}
            />
            {profile.identityFile ? (
              <HStack as="span" align="stretch" className="settings-appear">
                <IconButton title="Use ssh-agent" icon="xmark.circle" onClick={() => setProfile({ ...profile, identityFile: null })} />
              </HStack>
            ) : null}
          </div>
          {field('Codync port', remotePort, setRemotePort, '')}
        </CardSection>
        <Text size="caption" tone="secondary">
          {tr(`Codync opens an SSH tunnel to codync-host on that computer's loopback. It uses your SSH config and ssh-agent; agent forwarding stays off.`)}
        </Text>
        <Reveal show={problem !== null}>
          <Text size="caption" tone="danger">{problem}</Text>
        </Reveal>
      </VStack>
    </VStack>
  )
}
