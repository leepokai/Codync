import { useEffect, useState, type ReactNode } from 'react'
import { Button, CardSection, IconButton, RowLabel, Spinner, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { machineStatus, type CloudMachine } from '../../lib/cloud-computer'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useAccount } from '../../store/account'
import type { CloudComputerAction, CloudComputerModel } from '../../store/cloud-computer'
import type { BotStore } from '../../store/bot-store'
import { useApp } from '../../store/context'
import { CodyncVersionRow } from './CodyncVersionRow'

type Confirming = Exclude<CloudComputerAction, 'create'> | null

/**
 * The account's cloud computer: create it, or see where it runs and delete its snapshots or itself. Shown only
 * to a signed-in account that may have one or has one.
 */
export function CodyncCloudSection() {
  const app = useApp()
  const account = useAccount()
  const model = useModel(app.cloudComputer)
  const [confirming, setConfirming] = useState<Confirming>(null)
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
        accessory={<IconButton title="Refresh" icon="arrow.clockwise" onClick={() => void model.refresh()} />}
        footer={model.error ? <Text size="caption" tone="danger">{model.error}</Text> : null}
      >
        {model.machine ? (
          machineRows(model.machine, model, store, tr, setConfirming)
        ) : (
          <HStack gap={12} className="settings-row">
            <Text size="callout" tone="secondary" grow>{tr('A computer Codync runs for you in the cloud. It sleeps when idle and wakes when you message it.')}</Text>
            {model.busy === 'create' ? <Spinner /> : <Button onClick={() => void model.create()} disabled={model.busy !== null}>Create cloud computer</Button>}
          </HStack>
        )}
      </CardSection>
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
      <Text grow>{tr('Runs on {provider} · {region}').replace('{provider}', machine.providerName).replace('{region}', machine.region)}</Text>
    </HStack>,
    <ValueRow key="state" label="State">
      <Text tone={status.error ? 'danger' : 'secondary'}>{tr(status.text)}</Text>
    </ValueRow>,
    ...(store ? [<CodyncVersionRow key="codync" store={store} />] : []),
    deleteRow('deleteSnapshots', 'Delete snapshots'),
    deleteRow('delete', 'Delete cloud computer'),
  ]
}
