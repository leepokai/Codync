import { useEffect, useState, type ReactNode } from 'react'
import { BetaTag, Button, CardSection, IconButton, RowLabel, Spinner, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { machineStatus, placementLine, type CloudMachine } from '../../lib/cloud-computer'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import { useAccount } from '../../store/account'
import type { CloudComputerModel } from '../../store/cloud-computer'
import type { BotStore } from '../../store/bot-store'
import { useApp } from '../../store/context'
import { CloudKeyRows } from './CloudKeyRows'
import { CodyncVersionRow } from './CodyncVersionRow'

type Confirming = 'deleteSnapshots' | 'delete' | null

/**
 * The key to the user's own Boat account, then the account's cloud computer: create it, or see where it runs and
 * delete its snapshots or itself. Shown only to a signed-in account that may have one or has one.
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
        accessory={
          <>
            {/* Codync Cloud is a beta feature. */}
            <BetaTag />
            <IconButton title="Refresh" icon="arrow.clockwise" onClick={() => void model.refresh()} />
          </>
        }
        footer={
          <Text size="caption" tone={model.error ? 'danger' : 'secondary'}>
            {tr(model.error ?? "Runs on your own Boat account, which bills you while it's awake. It sleeps when idle and wakes when you message it.")}
          </Text>
        }
      >
        {model.machine
          ? [...machineRows(model.machine, model, store, tr, setConfirming), <CloudKeyRows key="key" model={model} canRemove={false} />]
          : [
              <CloudKeyRows key="key" model={model} canRemove />,
              ...(model.canCreate
                ? [
                    <HStack key="create" gap={12} className="settings-row">
                      <RowLabel grow label="Create cloud computer" />
                      {model.busy === 'create' ? <Spinner /> : <Button onClick={() => void model.create()} disabled={model.busy !== null}>Create</Button>}
                    </HStack>,
                  ]
                : []),
            ]}
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
