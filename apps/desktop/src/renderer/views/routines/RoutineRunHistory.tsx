import { useState } from 'react'
import { Button } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { runLabel, type RoutineRun } from './routine'
import './routine-run-history.css'

/** Inspect the host's recorded metadata, without rerunning work or borrowing today's instruction. */
export function RoutineRunHistory({ runs, name }: { runs: RoutineRun[]; name: string }) {
  const tr = useTr()
  const [expanded, setExpanded] = useState(false)
  const [selected, setSelected] = useState<string | null>(null)
  const current = runs.find((run) => run.id === selected)
  const index = runs.findIndex((run) => run.id === selected)
  const when = (time: number) => new Date(time).toLocaleString()
  return <VStack gap={8} className="routine-run-history">
    <Button kind="secondary" aria-expanded={expanded} title={`${tr('Run history')}: ${name}`} onClick={() => setExpanded((value) => !value)}>Run history</Button>
    <div className="routine-history-body" data-open={expanded} aria-hidden={!expanded} inert={!expanded}>
      <div className="routine-history-content">
      {!runs.length ? <Text size="caption" tone="secondary">No runs recorded yet.</Text> : current ? <VStack gap={8}>
        <HStack wrap gap={8}>
          <Button kind="secondary" onClick={() => setSelected(null)}>Back to runs</Button>
          <Button kind="secondary" disabled={index >= runs.length - 1} onClick={() => setSelected(runs[index + 1].id)}>Older run</Button>
          <Button kind="secondary" disabled={index <= 0} onClick={() => setSelected(runs[index - 1].id)}>Newer run</Button>
        </HStack>
        <Text weight="semibold">{tr(runLabel(current))}</Text>
        <Text size="caption" tone="secondary">{tr('Created')}: {when(current.createdAt)}</Text>
        <Text size="caption" tone="secondary">{tr('Finished')}: {current.finishedAt ? when(current.finishedAt) : tr('Not finished')}</Text>
        {current.scheduledFor ? <Text size="caption" tone="secondary">{tr('Scheduled for')}: {when(current.scheduledFor)}</Text> : null}
        {current.missed && current.missed.count > 0 ? (
          <Text size="caption" tone="secondary">
            {tr('Missed occurrences')}: {current.missed.count}
            {current.missed.until ? ` · ${tr('last at')} ${when(current.missed.until)}` : ''}
          </Text>
        ) : null}
        {current.notBefore ? <Text size="caption" tone="secondary">{tr('Retry scheduled')}: {when(current.notBefore)}</Text> : null}
        {current.detail ? <Text as="div" size="callout" className="routine-run-detail">{current.detail}</Text> : <Text size="caption" tone="secondary">No additional detail was recorded.</Text>}
      </VStack> : <VStack gap={6}>
        {runs.slice(0, 10).map((run) => <Button key={run.id} kind="secondary" onClick={() => setSelected(run.id)}>{when(run.createdAt)} · {tr(runLabel(run))}</Button>)}
      </VStack>}
      </div>
    </div>
  </VStack>
}
