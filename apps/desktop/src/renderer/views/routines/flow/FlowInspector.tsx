import { useEffect, useState } from 'react'
import type { Bot } from '@shared/models'
import { Button, DropdownMenu, Switch } from '../../../components/Controls'
import { HStack, Spacer, VStack } from '../../../components/Stack'
import { Text } from '../../../components/Text'
import { useTr } from '../../../lib/use-language'
import { botKey, latestStep, statusLabel, type FlowCanvas, type FlowLink, type FlowRoutineCard } from '../../../lib/routine-flow.ts'
import { runLabel } from '../routine'
import type { FlowSelection } from './FlowCanvas'
import { StatusChip } from './FlowNodeCard'

interface Props {
  canvas: FlowCanvas
  bots: ReadonlyMap<string, Bot>
  selection: FlowSelection
  busy: boolean
  offline: boolean
  onSaveInstruction: (link: FlowLink, instruction: string) => void
  onDisconnect: (link: FlowLink) => void
  onTest: (routineId: string) => void
  onSetEnabled: (routineId: string, enabled: boolean) => void
  onConnectFrom: (fromBotId: string, toBotId: string) => void
  onRemoveNode: (nodeKey: string) => void
  /** The bots `from` may hand a reply to, in some routine of this canvas. */
  targets: (fromBotId: string) => Bot[]
}

/** The side panel: what the selected routine, bot or link says, and the actions on it. */
export function FlowInspector(props: Props) {
  const { canvas, bots, selection } = props
  const tr = useTr()
  if (!selection) {
    return (
      <VStack gap={6} className="flow-inspector">
        <Text weight="semibold">{tr('Routine flow')}</Text>
        <Text size="callout" tone="secondary">{tr('Select a routine, a bot or a link. Drag from a bot\'s dot to another bot to hand its reply on.')}</Text>
      </VStack>
    )
  }
  if (selection.kind === 'edge') {
    const link = canvas.links.find((l) => l.id === selection.id)
    return link ? <LinkPanel {...props} link={link} bots={bots} /> : null
  }
  const [kind, id] = selection.key.split(':')
  if (kind === 'routine') {
    const card = canvas.routines.find((c) => c.routine.id === id)
    return card ? <RoutinePanel {...props} card={card} /> : null
  }
  const bot = bots.get(id)
  return <BotPanel {...props} botId={id} bot={bot} />
}

function LinkPanel({ link, bots, busy, offline, onSaveInstruction, onDisconnect }: Props & { link: FlowLink }) {
  const tr = useTr()
  const [text, setText] = useState(link.instruction)
  useEffect(() => setText(link.instruction), [link.id, link.instruction])
  const name = (id: string) => bots.get(id)?.name ?? tr('Unknown bot')
  return (
    <VStack gap={10} className="flow-inspector">
      <Text weight="semibold">{name(link.fromBotId)} → {name(link.toBotId)}</Text>
      <Text size="caption" tone="secondary">{tr('What the next bot is asked to do with the reply. It can be empty.')}</Text>
      <textarea
        className="routine-input flow-instruction"
        aria-label={tr('Instruction')}
        value={text}
        maxLength={8000}
        disabled={offline}
        onChange={(event) => setText(event.target.value)}
      />
      <HStack gap={8}>
        <Button disabled={busy || offline || text.trim() === link.instruction} onClick={() => onSaveInstruction(link, text.trim())}>{tr('Save')}</Button>
        <Spacer />
        <Button kind="secondary" disabled={busy || offline} onClick={() => onDisconnect(link)}>{tr('Disconnect')}</Button>
      </HStack>
    </VStack>
  )
}

function RoutinePanel({ card, canvas, busy, offline, onTest, onSetEnabled }: Props & { card: FlowRoutineCard }) {
  const tr = useTr()
  const { routine } = card
  const last = card.recentRuns[0]
  const linked = canvas.links.some((l) => l.routineId === routine.id)
  return (
    <VStack gap={10} className="flow-inspector">
      <Text weight="semibold">{routine.name}</Text>
      <Text size="callout" tone="secondary">{routine.triggerDescriptions.join(' · ')}</Text>
      <HStack gap={8}>
        <Text size="callout">{routine.enabled ? tr('Active') : tr('Paused')}</Text>
        <Spacer />
        <Switch on={routine.enabled} disabled={busy || offline} label={routine.name} onChange={(on) => onSetEnabled(routine.id, on)} />
      </HStack>
      {routine.nextRunAt ? <Text size="caption" tone="secondary">{tr('Next run')}: {new Date(routine.nextRunAt).toLocaleString()}</Text> : null}
      <Text size="caption" tone="secondary">{last ? `${tr('Last run')}: ${tr(runLabel(last))}` : tr('No runs yet')}</Text>
      {linked ? null : <Text size="caption" tone="secondary">{tr('Not connected')}</Text>}
      <Button disabled={busy || offline || !linked} onClick={() => onTest(routine.id)}>{tr('Test the flow')}</Button>
      <Steps card={card} />
      <History card={card} />
    </VStack>
  )
}

/** The steps of the latest run, one line each, with a bot's error or the start of its answer. */
function Steps({ card }: { card: FlowRoutineCard }) {
  const tr = useTr()
  if (card.steps.length === 0) return null
  return (
    <VStack gap={6}>
      <Text size="caption" tone="secondary" weight="semibold">{tr('Latest run')}</Text>
      {card.steps.map((step) => (
        <VStack key={step.id} gap={2} className="flow-step">
          <HStack gap={8}>
            <Text size="callout" truncate>{step.botId}</Text>
            <Spacer />
            <StatusChip status={step.status} />
          </HStack>
          {step.error ? <Text size="caption" tone="danger" lines={3}>{step.error}</Text> : null}
          {step.output ? <Text size="caption" tone="secondary" lines={3}>{step.output}</Text> : null}
        </VStack>
      ))}
    </VStack>
  )
}

function History({ card }: { card: FlowRoutineCard }) {
  const tr = useTr()
  if (card.recentRuns.length === 0) return null
  return (
    <VStack gap={4}>
      <Text size="caption" tone="secondary" weight="semibold">{tr('Run history')}</Text>
      {card.recentRuns.map((run) => (
        <HStack key={run.id} gap={8}>
          <Text size="caption">{tr(runLabel(run))}</Text>
          <Spacer />
          <Text size="caption" tone="secondary">{new Date(run.createdAt).toLocaleString()}</Text>
        </HStack>
      ))}
    </VStack>
  )
}

function BotPanel({ botId, bot, canvas, busy, offline, targets, onConnectFrom, onRemoveNode }: Props & { botId: string; bot?: Bot }) {
  const tr = useTr()
  const step = latestStep(canvas, botId)
  const options = targets(botId)
  const removable = canvas.placedBotIds.includes(botId) && !canvas.links.some((l) => l.fromBotId === botId || l.toBotId === botId)
  return (
    <VStack gap={10} className="flow-inspector">
      <Text weight="semibold">{bot?.name ?? tr('Unknown bot')}</Text>
      <Text size="callout" tone="secondary">{bot?.model || bot?.backend || ''}</Text>
      <HStack gap={8}>
        <StatusChip status={step?.status ?? null} />
        <Text size="caption" tone="secondary">{tr(statusLabel(step?.status ?? null))}</Text>
      </HStack>
      <DropdownMenu
        title={tr('Hand its reply on to…')}
        className="flow-menu-button"
        items={() => options.map((target) => ({ title: target.name, action: () => onConnectFrom(botId, target.id) }))}
      >
        <Text>{options.length === 0 ? tr('No bot to hand on to') : tr('Hand its reply on to…')}</Text>
      </DropdownMenu>
      {removable ? <Button kind="secondary" disabled={busy || offline} onClick={() => onRemoveNode(botKey(botId))}>{tr('Remove from the canvas')}</Button> : null}
      {step?.error ? <Text size="caption" tone="danger" lines={4}>{step.error}</Text> : null}
    </VStack>
  )
}
