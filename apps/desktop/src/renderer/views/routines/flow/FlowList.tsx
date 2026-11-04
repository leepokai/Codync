import type { KeyboardEvent } from 'react'
import type { Bot } from '@shared/models'
import { HStack, Spacer, VStack } from '../../../components/Stack'
import { Text } from '../../../components/Text'
import { useTr } from '../../../lib/use-language'
import { botKey, flowRows, isUnconnected, routineKey, statusLabel, type FlowCanvas } from '../../../lib/routine-flow.ts'
import type { FlowSelection } from './FlowCanvas'
import { StatusChip } from './FlowNodeCard'

/**
 * The canvas as a list, routine by routine and hop by hop: what the keyboard and a screen reader
 * walk (arrows move between rows, Enter selects). It shows the same things as the canvas and the
 * selection is shared, so the inspector works from either.
 */
export function FlowList({ canvas, bots, selection, onSelect }: {
  canvas: FlowCanvas
  bots: ReadonlyMap<string, Bot>
  selection: FlowSelection
  onSelect: (selection: FlowSelection) => void
}) {
  const tr = useTr()
  const rows = flowRows(canvas)
  const name = (id: string) => bots.get(id)?.name ?? tr('Unknown bot')
  const walk = (event: KeyboardEvent) => {
    const step = event.key === 'ArrowDown' ? 1 : event.key === 'ArrowUp' ? -1 : 0
    if (!step) return
    event.preventDefault()
    const items = [...(event.currentTarget.parentElement?.querySelectorAll<HTMLElement>('[data-row]') ?? [])]
    items[items.indexOf(event.currentTarget as HTMLElement) + step]?.focus()
  }
  return (
    <VStack gap={14} className="flow-list" role="list" aria-label={tr('Routine flow')}>
      {canvas.routines.map(({ routine }) => (
        <VStack key={routine.id} gap={4} role="listitem">
          <button
            data-row
            className="flow-row"
            data-selected={selection?.kind === 'node' && selection.key === routineKey(routine.id)}
            onKeyDown={walk}
            onClick={() => onSelect({ kind: 'node', key: routineKey(routine.id) })}
          >
            <Text weight="semibold" truncate>{routine.name}</Text>
            <Spacer />
            {isUnconnected(canvas, routine.id) ? <Text size="caption" tone="secondary">{tr('Not connected')}</Text> : null}
          </button>
          {rows.filter((row) => row.routineId === routine.id).map((row) => (
            <button
              key={row.key}
              data-row
              className="flow-row"
              data-selected={row.link ? selection?.kind === 'edge' && selection.id === row.link.id : selection?.kind === 'node' && selection.key === botKey(row.botId)}
              aria-label={`${name(row.botId)}, ${tr(statusLabel(row.step?.status ?? null))}`}
              onKeyDown={walk}
              onClick={() => onSelect(row.link ? { kind: 'edge', id: row.link.id } : { kind: 'node', key: botKey(row.botId) })}
            >
              <HStack gap={8} shrink className="flow-row-indent" data-depth={row.depth}>
                <Text truncate>{row.depth > 0 ? '→ ' : ''}{name(row.botId)}</Text>
                {row.link?.instruction ? <Text size="caption" tone="secondary" truncate>{row.link.instruction}</Text> : null}
              </HStack>
              <Spacer />
              <StatusChip status={row.step?.status ?? null} />
            </button>
          ))}
        </VStack>
      ))}
    </VStack>
  )
}
