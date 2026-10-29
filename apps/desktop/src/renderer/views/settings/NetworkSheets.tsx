import { useCallback, useEffect, useState } from 'react'
import { Button, CardSection, ChoiceList, IconButton, Reveal, Spinner } from '../../components/Controls'
import { ModalHeader, useDismiss } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { connectorChoices, connectorFor, expiryTime, type ConnectorChoice } from '../../lib/network-copy'
import { useTr } from '../../lib/use-language'
import type { Network, RecoveryCode } from '@shared/network'
import type { BotStore } from '../../store/bot-store'
import { network } from '../../store/network'
import { errorText } from './parts'

const NETWORK_ICON = 'point.3.connected.trianglepath.dotted'

function Field({ label, value, set, placeholder, mono = false }: { label: string; value: string; set: (v: string) => void; placeholder: string; mono?: boolean }) {
  const tr = useTr()
  return (
    <HStack gap={12} className="settings-row">
      <Text tone="text">{tr(label)}</Text>
      <input
        className={`settings-field${mono ? ' settings-mono' : ''}`}
        aria-label={tr(label)}
        placeholder={tr(placeholder)}
        value={value}
        spellCheck={false}
        onChange={(e) => set(e.target.value)}
      />
    </HStack>
  )
}

/** Create a network (this computer its owner) or join one with an invite: one more, a computer can be in several. */
export function NetworkSetupSheet({ store, joining }: { store: BotStore; joining: boolean }) {
  const tr = useTr()
  const dismiss = useDismiss()
  const [invite, setInvite] = useState('')
  const [networkName, setNetworkName] = useState('')
  const [name, setName] = useState('')
  const [choice, setChoice] = useState<ConnectorChoice>('public')
  const [url, setUrl] = useState('')
  const [busy, setBusy] = useState(false)
  const [problem, setProblem] = useState<string | null>(null)

  const save = async () => {
    if (busy) return
    if (joining && !invite.trim()) return setProblem(tr('Paste the invite from the other computer.'))
    const connector = connectorFor(choice, url)
    if ('problem' in connector) return setProblem(tr(connector.problem))
    setBusy(true)
    setProblem(null)
    try {
      if (joining) await network.join(store, invite, networkName, name, connector.connector)
      else await network.create(store, networkName, name, connector.connector)
      dismiss()
    } catch (e) {
      setProblem(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  return (
    <VStack className="network-sheet" onKeyDown={(e) => e.key === 'Enter' && e.target instanceof HTMLInputElement && void save()}>
      <ModalHeader
        icon={NETWORK_ICON}
        title={joining ? 'Join with invite' : 'Create network'}
        trailing={busy ? <Spinner /> : <IconButton title={joining ? 'Join' : 'Create'} icon="checkmark" onClick={() => void save()} />}
      />
      <VStack gap={16} className="network-sheet-body">
        <CardSection footer={joining ? null : 'This computer becomes the owner: it invites the others.'}>
          {[
            ...(joining ? [<Field key="invite" label="Invite" value={invite} set={setInvite} placeholder="codync-network:…" mono />] : []),
            <Field key="network" label="Network name" value={networkName} set={setNetworkName} placeholder={joining ? 'From the invite' : 'home'} />,
            <Field key="name" label="Computer name" value={name} set={setName} placeholder="Host name" />,
          ]}
        </CardSection>
        <CardSection title="When there's no direct path">
          <ChoiceList selection={choice} options={connectorChoices} onChange={setChoice} />
          {choice === 'relay' ? <Field key="url" label="Relay address" value={url} set={setUrl} placeholder="https://" /> : null}
        </CardSection>
        <Reveal show={problem !== null}>
          <Text size="caption" tone="danger">{problem}</Text>
        </Reveal>
      </VStack>
    </VStack>
  )
}

/** The recovery code, once: closing it is confirming it was saved. The host keeps it nowhere. */
export function RecoveryCodeSheet({ recovery: { code, network: networkName, computer }, done }: { recovery: RecoveryCode; done: () => void }) {
  const tr = useTr()
  return (
    <VStack className="network-sheet">
      <ModalHeader icon="key" title="Save your recovery code" onClose={null} />
      <VStack gap={14} className="network-sheet-body">
        <HStack gap={8}>
          <Text as="div" className="settings-code network-code" mono>{code}</Text>
          <IconButton title="Copy recovery code" icon="doc.on.doc" onClick={() => window.codync.app.copy(code)} />
        </HStack>
        <Text size="callout" tone="secondary">
          {tr("It makes another computer the owner of {network} if {name} is lost. It's shown only once and kept on no computer.")
            .replaceAll('{network}', networkName)
            .replaceAll('{name}', computer)}
        </Text>
        <HStack justify="end">
          <Button onClick={done}>I saved it</Button>
        </HStack>
      </VStack>
    </VStack>
  )
}

/** A one-time invite to `net` for another computer, good for an hour while this one stays on. */
export function InviteSheet({ store, net }: { store: BotStore; net: Network }) {
  const tr = useTr()
  const [invite, setInvite] = useState<{ invite: string; expiresAt: number } | { error: string } | null>(null)
  const make = useCallback(async () => {
    setInvite(null)
    try {
      setInvite(await network.invite(store, net.id))
    } catch (e) {
      setInvite({ error: errorText(e) })
    }
  }, [store, net.id])
  useEffect(() => void make(), [make])

  return (
    <VStack className="network-sheet">
      <ModalHeader icon="person.crop.circle.badge.plus" title="Invite a computer" subtitle={net.name} trailing={<IconButton title="New invite" icon="arrow.clockwise" onClick={() => void make()} />} />
      <VStack gap={14} className="network-sheet-body">
        {invite === null ? (
          <Spinner />
        ) : 'error' in invite ? (
          <Text tone="danger">{invite.error}</Text>
        ) : (
          <>
            <HStack gap={8}>
              <Text as="div" className="settings-code network-code" mono>{invite.invite}</Text>
              <IconButton title="Copy invite" icon="doc.on.doc" onClick={() => window.codync.app.copy(invite.invite)} />
            </HStack>
            <Text size="callout" tone="secondary">
              {tr('One use. Works until {time} while {name} stays on.').replaceAll('{time}', expiryTime(invite.expiresAt)).replaceAll('{name}', net.computer)}
            </Text>
            <Text size="callout" tone="secondary">{tr('On the other computer, choose Join with invite under Network.')}</Text>
          </>
        )}
      </VStack>
    </VStack>
  )
}

/** Takes ownership of `net` with its recovery code, for when the owner was lost. */
export function RecoverSheet({ store, net }: { store: BotStore; net: Network }) {
  const tr = useTr()
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null)
  const recover = async () => {
    if (busy || !code.trim()) return
    setBusy(true)
    try {
      await network.recover(store, net.id, code)
      setResult({ ok: true, text: tr('{name} is the owner now. Remove the lost computer from the list; if it was stolen, also remove the computers it invited.').replaceAll('{name}', net.computer) })
    } catch (e) {
      setResult({ ok: false, text: errorText(e) })
    } finally {
      setBusy(false)
    }
  }

  return (
    <VStack className="network-sheet" onKeyDown={(e) => e.key === 'Enter' && e.target instanceof HTMLInputElement && void recover()}>
      <ModalHeader
        icon="key"
        title="Recover ownership"
        subtitle={net.name}
        trailing={busy ? <Spinner /> : result?.ok ? null : <IconButton title="Recover" icon="checkmark" onClick={() => void recover()} />}
      />
      <VStack gap={16} className="network-sheet-body">
        <CardSection footer="For when the owner is lost: this computer becomes the owner.">
          <Field label="Recovery code" value={code} set={setCode} placeholder="00000000-…" mono />
        </CardSection>
        <Reveal show={result !== null}>
          <Text size="caption" tone={result?.ok ? 'secondary' : 'danger'}>{result?.text}</Text>
        </Reveal>
      </VStack>
    </VStack>
  )
}
