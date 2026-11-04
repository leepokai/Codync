import type { KeyboardEvent, PointerEvent } from 'react'
import type { Bot } from '@shared/models'
import { BotAvatar } from '../../../components/Avatar'
import { Icon } from '../../../components/Icon'
import { HStack, VStack } from '../../../components/Stack'
import { Text } from '../../../components/Text'
import { useTr } from '../../../lib/use-language'
import { statusLabel, type FlowNode, type FlowRoutineCard, type FlowStep, type StepStatus } from '../../../lib/routine-flow.ts'

const STATUS_ICON: Record<StepStatus, string> = {
  running: 'arrow.triangle.2.circlepath',
  succeeded: 'checkmark.circle',
  failed: 'xmark.circle',
  skipped: 'minus.circle',
  cancelled: 'stop.circle',
}

/** A step's status as a small chip: icon and words, never color alone. */
export function StatusChip({ status }: { status: StepStatus | null }) {
  const tr = useTr()
  return (
    <HStack as="span" gap={4} className="flow-status" data-status={status ?? 'none'}>
      <Icon name={status ? STATUS_ICON[status] : 'circle.dashed'} size={11} />
      <Text size="caption">{tr(statusLabel(status))}</Text>
    </HStack>
  )
}

interface Props {
  node: FlowNode
  /** Where the card stands now (a drag follows the pointer before it is saved). */
  x: number
  y: number
  bot?: Bot
  routine?: FlowRoutineCard
  step: FlowStep | null
  unconnected: boolean
  selected: boolean
  connectTarget: boolean
  disabled: boolean
  onSelect: () => void
  onDragStart: (event: PointerEvent) => void
  onPortStart: (event: PointerEvent) => void
  onKey: (event: KeyboardEvent) => void
}

/** One node of the canvas: a routine, or a bot with its avatar, model and the status of its latest step. */
export function FlowNodeCard({ node, x, y, bot, routine, step, unconnected, selected, connectTarget, disabled, onSelect, onDragStart, onPortStart, onKey }: Props) {
  const tr = useTr()
  const title = node.kind === 'routine' ? (routine?.routine.name ?? '') : (bot?.name ?? tr('Unknown bot'))
  return (
    <div
      className="flow-node"
      data-kind={node.kind}
      data-selected={selected}
      data-target={connectTarget}
      role="button"
      tabIndex={0}
      aria-label={`${tr(node.kind === 'routine' ? 'Routine' : 'Bot')}: ${title}`}
      aria-pressed={selected}
      style={{ transform: `translate(${x}px, ${y}px)` }}
      onPointerDown={(event) => {
        onSelect()
        if (!disabled) onDragStart(event)
      }}
      onKeyDown={onKey}
    >
      <HStack gap={10} align="center" className="flow-node-head">
        {node.kind === 'routine' ? (
          <Icon name="clock.arrow.circlepath" size={26} />
        ) : bot ? (
          <BotAvatar bot={bot} size={30} animated={false} />
        ) : (
          <Icon name="questionmark.circle" size={26} />
        )}
        <VStack gap={2} shrink>
          <Text weight="semibold" truncate>{title}</Text>
          {node.kind === 'bot' ? (
            <Text size="caption" tone="secondary" truncate>{bot?.model || bot?.backend || ''}</Text>
          ) : (
            <Text size="caption" tone="secondary" truncate>{routine?.routine.triggerDescriptions.join(' · ')}</Text>
          )}
        </VStack>
      </HStack>
      {node.kind === 'bot' ? (
        <StatusChip status={step?.status ?? null} />
      ) : (
        <Text size="caption" tone="secondary">
          {unconnected ? tr('Not connected') : routine?.steps.length ? tr('Latest run') : tr('No runs yet')}
        </Text>
      )}
      {node.kind === 'bot' ? (
        <button
          type="button"
          className="flow-port"
          title={tr('Drag to another bot to hand its reply on')}
          aria-label={tr('Connect from this bot')}
          disabled={disabled}
          onPointerDown={(event) => {
            event.stopPropagation()
            onPortStart(event)
          }}
        />
      ) : null}
    </div>
  )
}
