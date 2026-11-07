import { useEffect, useState, type ReactNode } from 'react'
import { BetaTag, Button, CardSection, DropdownMenu, IconButton, RowLabel, Spinner, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog, Sheet } from '../../components/Overlay'
import { HStack, Spacer } from '../../components/Stack'
import { Text } from '../../components/Text'
import { machineStatus, placementLine, providersSummary, type CloudMachine } from '../../lib/cloud-computer'
import { fill } from '../../lib/fill'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useAccount } from '../../store/account'
import type { CloudComputerModel } from '../../store/cloud-computer'
import type { BotStore } from '../../store/bot-store'
import { useApp } from '../../store/context'
import { CloudProvidersSheet } from './CloudProvidersSheet'
import { CodyncVersionRow } from './CodyncVersionRow'

type Confirming = 'deleteSnapshots' | 'delete' | null

/**
 * The account's cloud computer on the user's own provider account: where it runs, deleting its snapshots or itself, or
 * creating it on a connected provider; and the Cloud providers sheet with their keys. Shown only to a signed-in
 * account that may have one or has one.
 */
export function CodyncCloudSection() {
  const app = useApp()
  const account = useAccount()
  const model = useModel(app.cloudComputer)
  const [confirming, setConfirming] = useState<Confirming>(null)
  const [providersOpen, setProvidersOpen] = useState(false)
  const tr = useTr()
  const userId = account.user?.userId
  // Once this app reaches it: the Codync it runs, and the owner's update to the latest release.
  const store = useModel(model.machine?.computerId ? app.store(model.machine.computerId) : null)
  useEffect(() => void (userId && app.cloudComputer.refresh()), [app, userId])
  if (!userId || !model.shown) return null
  return (
    <>
      <CardSection
        title="Codync Cloud"
        accessory={
          <>
            {/* Codync Cloud is a beta feature. */}
            <BetaTag />
            <IconButton title="Refresh" icon="arrow.clockwise" onClick={() => void model.refresh()} />
          </>
        }
        footer={
          <Text size="caption" tone={model.error && !providersOpen ? 'danger' : 'secondary'}>
            {tr(model.error && !providersOpen ? model.error : "Runs on your own account with a cloud provider, which bills you while it's awake. It sleeps when idle and wakes when you message it.")}
          </Text>
        }
      >
        {[
          ...(model.machine ? machineRows(model.machine, model, store, tr, setConfirming) : []),
          <ProvidersRow key="providers" model={model} open={() => setProvidersOpen(true)} />,
          ...(!model.machine && model.connected.length ? [<CreateRow key="create" model={model} />] : []),
        ]}
      </CardSection>
      <Sheet open={providersOpen} onClose={() => setProvidersOpen(false)}>
        <CloudProvidersSheet model={model} />
      </Sheet>
      <Dialog
        open={confirming === 'deleteSnapshots'}
        title={tr('Delete snapshots?')}
        message={tr("Erases your cloud computer's earlier saved states and its backup. The computer and what's on it now stay.")}
        actions={[{ title: 'Delete snapshots', destructive: true, action: () => void model.deleteSnapshots() }]}
        onClose={() => setConfirming(null)}
      />
      <Dialog
        open={confirming === 'delete'}
        title={tr('Delete your cloud computer?')}
        message={tr("Erases everything on it: bots, chats, files and sign-ins. This can't be undone.")}
        actions={[{ title: 'Delete', destructive: true, action: () => void model.delete() }]}
        onClose={() => setConfirming(null)}
      />
    </>
  )
}

/** One card row each (CardSection puts hairlines between its children). */
function machineRows(machine: CloudMachine, model: CloudComputerModel, store: BotStore | null, tr: (s: string) => string, confirm: (c: Confirming) => void): ReactNode[] {
  const status = machineStatus(machine)
  const deleteRow = (action: Exclude<Confirming, null>, label: string) => (
    <HStack key={action} gap={12} className="settings-row">
      <RowLabel grow label={label} />
      {model.busy === action ? <Spinner /> : <IconButton title={label} icon="trash" onClick={() => confirm(action)} disabled={model.busy !== null} />}
    </HStack>
  )
  return [
    <HStack key="machine" gap={10} className="settings-row">
      <Icon name="cloud" size={16} color="var(--secondary)" />
      <Text grow>{placementLine(machine, tr)}</Text>
    </HStack>,
    <ValueRow key="state" label="State">
      <Text tone={status.error ? 'danger' : 'secondary'}>{tr(status.text)}</Text>
    </ValueRow>,
    ...(store ? [<CodyncVersionRow key="codync" store={store} />] : []),
    deleteRow('deleteSnapshots', 'Delete snapshots'),
    deleteRow('delete', 'Delete cloud computer'),
  ]
}

/** Opens the Cloud providers sheet; says which providers have a key, or that one needs attention. */
function ProvidersRow({ model, open }: { model: CloudComputerModel; open: () => void }) {
  const tr = useTr()
  const summary = providersSummary(model.providers)
  return (
    <button className="settings-row cloud-providers-row" onClick={open}>
      <Icon name="key.fill" size={14} color="var(--secondary)" />
      <Text tone="text">{tr('Cloud providers')}</Text>
      <Spacer min={12} />
      <Text tone={summary.error ? 'danger' : 'secondary'} truncate>{tr(summary.text)}</Text>
      <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
    </button>
  )
}

/** Creates the computer on the one connected provider, or on the one picked from a menu. */
function CreateRow({ model }: { model: CloudComputerModel }) {
  const tr = useTr()
  const [only] = model.connected
  const busy = model.busy !== null
  return (
    <HStack gap={12} className="settings-row">
      <RowLabel grow label="Create cloud computer" />
      {model.busy === 'create' ? (
        <Spinner />
      ) : model.connected.length === 1 && only ? (
        <Button onClick={() => void model.create(only.provider)} disabled={busy}>Create</Button>
      ) : (
        <DropdownMenu
          className="styled-button primary"
          items={() => model.connected.map((p) => ({ title: fill(tr('On {provider}'), { provider: p.name }), verbatim: true, action: () => void model.create(p.provider) }))}
        >
          {tr('Create')}
        </DropdownMenu>
      )}
    </HStack>
  )
}
