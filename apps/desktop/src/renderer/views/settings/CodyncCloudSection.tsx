import { useEffect, useState } from 'react'
import { BetaTag, CardSection, DropdownMenu, IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, Sheet } from '../../components/Overlay'
import { ProviderIcon } from '../../components/ProviderIcon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { limitCloudName, machineActions, machineStatus, type CloudMachine, type MachineAction } from '../../lib/cloud-computer'
import { fill } from '../../lib/fill'
import { useModel, useModels } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useAccount } from '../../store/account'
import type { CloudComputerModel } from '../../store/cloud-computer'
import { useApp } from '../../store/context'
import { NameDialog, type Naming } from '../NameDialog'
import { CloudKeySheet } from './CloudKeySheet'
import { CodyncVersionRow } from './CodyncVersionRow'
import { NewCloudComputerSheet } from './NewCloudComputerSheet'

/** A cloud computer's delete waiting for its confirmation: the whole computer, or its snapshots. */
type Asking = { action: 'deleteSnapshots' | 'delete'; machine: CloudMachine } | null

const actionIcons: Record<MachineAction, string> = { rename: 'pencil', key: 'key.fill', deleteSnapshots: 'clock.arrow.circlepath', delete: 'trash' }

/**
 * The account's cloud computers on the user's own provider accounts, each with its name, where it runs and its state,
 * renamed, its provider's key opened, or deleted (or its snapshots) from its `⋯` menu, and the Codync it runs once this
 * app reaches it; "+" makes another on a provider picked there, whose key opens from there too. Shown only to a
 * signed-in account that may have them or has some.
 */
export function CodyncCloudSection() {
  const app = useApp()
  const account = useAccount()
  const model = useModel(app.cloudComputer)
  const [asking, setAsking] = useState<Asking>(null)
  const [naming, setNaming] = useState<Naming | null>(null)
  const [creating, setCreating] = useState(false)
  /** The provider whose key sheet is open. */
  const [keyFor, setKeyFor] = useState<string | null>(null)
  const tr = useTr()
  const userId = account.user?.userId
  // Once this app reaches one: the Codync it runs, and the owner's update to the latest release.
  const storeOf = (m: CloudMachine) => (m.computerId ? app.store(m.computerId) : null)
  useModels(model.machines.flatMap((m) => storeOf(m) ?? []))
  useEffect(() => void (userId && app.cloudComputer.refresh()), [app, userId])
  if (!userId || !model.shown) return null
  // Under the key sheet it shows there instead.
  const error = keyFor ? null : model.errorAt('card')
  const ask = (action: MachineAction, machine: CloudMachine) => {
    if (action === 'key') return setKeyFor(machine.provider)
    if (action !== 'rename') return setAsking({ action, machine })
    setNaming({ title: 'Rename cloud computer', name: machine.name, save: (name) => void model.rename(machine.cloudComputerId, name) })
  }
  // A failure from an earlier opening is over, a row's key removal too.
  const openNew = () => {
    model.clearError('create')
    for (const p of model.providers) model.clearError(`key:${p.provider}`)
    setCreating(true)
  }
  // Closed, its failure was seen: New cloud computer under it shows only a removal asked from its own row.
  const closeKey = () => {
    if (keyFor) model.clearError(`key:${keyFor}`)
    setKeyFor(null)
  }
  return (
    <>
      <CardSection
        title="Codync Cloud"
        accessory={
          <>
            {/* Codync Cloud is a beta feature. */}
            <BetaTag />
            <IconButton title="Refresh" icon="arrow.clockwise" onClick={() => void model.refresh()} />
            {/* At the limit too: the sheet says why it can't make another, and keeps every provider's key in reach. */}
            {model.available ? <IconButton title="New cloud computer" icon="plus" onClick={openNew} disabled={model.busy !== null} /> : null}
          </>
        }
        footer={
          <Text size="caption" tone={error ? 'danger' : 'secondary'}>
            {tr(error ?? "Runs on your own account with a cloud provider, which bills you while it's awake. It sleeps when idle and wakes when you message it.")}
          </Text>
        }
      >
        {model.machines.map((m) => {
          const store = storeOf(m)
          return [
            <MachineRow key={m.cloudComputerId} machine={m} model={model} ask={(action) => ask(action, m)} />,
            store?.hello ? <CodyncVersionRow key={`${m.cloudComputerId}:codync`} store={store} /> : null,
          ]
        })}
      </CardSection>
      <Sheet open={creating} onClose={() => setCreating(false)}>
        <NewCloudComputerSheet model={model} openKey={setKeyFor} />
      </Sheet>
      <Sheet open={keyFor !== null} onClose={closeKey}>
        {keyFor ? <CloudKeySheet model={model} provider={keyFor} done={() => setKeyFor(null)} /> : null}
      </Sheet>
      <Dialog
        open={asking?.action === 'deleteSnapshots'}
        title={tr('Delete snapshots?')}
        message={tr("Erases your cloud computer's earlier saved states and its backup. The computer and what's on it now stay.")}
        actions={asking ? [{ title: 'Delete snapshots', destructive: true, action: () => void model.deleteSnapshots(asking.machine.cloudComputerId) }] : []}
        onClose={() => setAsking(null)}
      />
      <Dialog
        open={asking?.action === 'delete'}
        title={fill(tr('Delete {name}?'), { name: asking?.machine.name ?? '' })}
        message={tr("Erases everything on it: bots, chats, files and sign-ins. This can't be undone.")}
        actions={asking ? [{ title: 'Delete', destructive: true, action: () => void model.delete(asking.machine.cloudComputerId) }] : []}
        onClose={() => setAsking(null)}
      />
      <NameDialog naming={naming} placeholder="Name" limit={limitCloudName} clean={(typed) => typed.trim() || null} onClose={() => setNaming(null)} />
    </>
  )
}

/** A cloud computer: its provider's logo, its name, where it runs and its state (or what to fix while its provider
 * turns the key down); a spinner while its action runs, and its menu off while another one does (one at a time). */
function MachineRow({ machine, model, ask }: { machine: CloudMachine; model: CloudComputerModel; ask: (action: MachineAction) => void }) {
  const tr = useTr()
  const status = machineStatus(machine, model.providers.find((p) => p.provider === machine.provider)?.credential?.problem)
  return (
    <HStack gap={10} className="settings-row">
      <ProviderIcon provider={machine.provider} size={20} />
      <VStack grow className="settings-stack">
        <Text tone="text" truncate>{machine.name}</Text>
        <Text size="caption" tone="secondary" truncate>
          {`${machine.providerName} · ${machine.region} · `}
          <Text tone={status.error ? 'danger' : undefined}>{tr(status.text)}</Text>
        </Text>
      </VStack>
      {model.isBusy(machine.cloudComputerId) ? (
        <Spinner />
      ) : (
        <DropdownMenu
          className="icon-button cloud-machine-menu"
          title={tr('More')}
          disabled={model.busy !== null}
          items={() =>
            machineActions(machine, tr).map(({ action, label, disabled }) => ({ title: label, verbatim: true, icon: actionIcons[action], destructive: action === 'delete', disabled, action: () => ask(action) }))
          }
        >
          <Icon name="ellipsis" size={14} weight="medium" />
        </DropdownMenu>
      )}
    </HStack>
  )
}
