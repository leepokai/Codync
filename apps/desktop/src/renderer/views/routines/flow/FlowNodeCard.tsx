import type { CSSProperties, KeyboardEvent, PointerEvent } from 'react'
import type { Bot } from '@shared/models'
import { BotAvatar } from '../../../components/Avatar'
import { Icon } from '../../../components/Icon'
import { HStack, VStack } from '../../../components/Stack'
import { Text } from '../../../components/Text'
import { useTr } from '../../../lib/use-language'
import { routineTint, statusLabel, type FlowNode, type FlowRoutineCard, type FlowStep, type StepStatus } from '../../../lib/routine-flow.ts'

const STATUS_ICON: Record<StepStatus, string> = {
  running: 'arrow.triangle.2.circlepath',
  succeeded: 'checkmark.circle',
  failed: 'xmark.circle',
  skipped: 'minus.circle',
  cancelled: 'stop.circle',
}

const DAY_MS = 24 * 60 * 60 * 1000

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

/** The week ahead as seven columns, a tick where the routine fires (OpenBot's routine card). */
function WeekStrip({ upcoming }: { upcoming: number[] }) {
  const tr = useTr()
  const start = new Date()
  start.setHours(0, 0, 0, 0)
  const days = Array.from({ length: 7 }, (_, index) => {
    const from = start.getTime() + index * DAY_MS
    const ticks = upcoming
      .filter((at) => at >= from && at < from + DAY_MS)
      .map((at) => ((at - from) / DAY_MS) * 100)
    return { from, ticks, today: index === 0 }
  })
  const count = days.reduce((total, day) => total + day.ticks.length, 0)
  if (count === 0) return null
  return (
    <div className="flow-week" role="img" aria-label={tr('Fires {count} times in the next seven days').replace('{count}', String(count))}>
      {days.map((day) => (
        <span key={day.from} className="flow-week-day" data-today={day.today ? 'true' : undefined}>
          <span className="flow-week-track">
            {day.ticks.map((left, index) => <span key={index} className="flow-week-tick" style={{ left: `${left}%` }} />)}
          </span>
        </span>
      ))}
    </div>
  )
}

/** The newest runs as dots, oldest on the left so the row reads forward in time. */
function RunDots({ runs }: { runs: FlowRoutineCard['recentRuns'] }) {
  const tr = useTr()
  const recent = runs.slice(0, 6).reverse()
  if (recent.length === 0) return null
  return (
    <span className="flow-runs" aria-label={tr('Recent runs')}>
      {recent.map((run) => (
        <span
          key={run.id}
          className="flow-run-dot"
          data-status={run.status}
          title={`${tr(statusLabel(run.status as StepStatus))} · ${new Date(run.createdAt).toLocaleString()}`}
        />
      ))}
    </span>
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
  /** Runs this routine's flow; only routine cards have it. */
  onRun?: () => void
}

/** One node of the canvas: a routine with its week and last runs, or a bot with its latest step. */
export function FlowNodeCard({ node, x, y, bot, routine, step, unconnected, selected, connectTarget, disabled, onSelect, onDragStart, onPortStart, onKey, onRun }: Props) {
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
      style={{ transform: `translate(${x}px, ${y}px)`, ...(node.kind === 'routine' ? { '--flow-tint': routineTint(node.id) } as CSSProperties : {}) }}
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
        {node.kind === 'routine' && onRun ? (
          <button
            type="button"
            className="flow-run-button"
            title={unconnected ? tr('Connect a bot first') : tr('Run this routine now')}
            aria-label={unconnected ? tr('Connect a bot first') : tr('Run this routine now')}
            disabled={disabled || routine?.routine.enabled === false || unconnected}
            onPointerDown={(event) => event.stopPropagation()}
            onClick={(event) => {
              event.stopPropagation()
              onRun()
            }}
          >
            <Icon name="play.fill" size={12} />
          </button>
        ) : null}
      </HStack>
      {node.kind === 'routine' ? (
        <>
          <WeekStrip upcoming={routine?.upcomingRuns ?? []} />
          <HStack gap={8} align="center" className="flow-node-foot">
            <RunDots runs={routine?.recentRuns ?? []} />
            <Text size="caption" tone="secondary" truncate>
              {unconnected ? tr('Not connected') : routine?.steps.length ? tr('Latest run') : tr('No runs yet')}
            </Text>
          </HStack>
        </>
      ) : (
        <StatusChip status={step?.status ?? null} />
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
