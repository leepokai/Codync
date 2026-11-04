import { useEffect, useState } from 'react'
import { Button, TabChoice } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { byAttention, CONFIDENCES, confidenceLabel, confirmedText, filterBeliefs, historyRows, KINDS, kindLabel, type Belief, type BeliefConfidence, type BeliefKind } from '../../../shared/beliefs'
import type { MemoryStore } from '../../store/memory-store'
import { relativeTime } from '../thread/chat-items'

const ALL = 'all'

/** "What I know about you": the bot's notes as beliefs, with how sure it is, filters, and a way to confirm, forget or bring back. */
export function BeliefsSection({ memory, rev }: { memory: MemoryStore; rev: number | undefined }) {
  const tr = useTr()
  const [kind, setKind] = useState<BeliefKind | typeof ALL>(ALL)
  const [confidence, setConfidence] = useState<BeliefConfidence | typeof ALL>(ALL)
  const [forgotten, setForgotten] = useState(false)
  const [open, setOpen] = useState<string | null>(null)
  useEffect(() => { void memory.loadBeliefs(forgotten) }, [memory, forgotten, rev])
  const rows = memory.beliefs
  if (rows === null) return <p className="memory-muted">{memory.beliefError ?? tr('Loading…')}</p>
  const shown = byAttention(filterBeliefs(rows, { kind: kind === ALL ? null : kind, confidence: confidence === ALL ? null : confidence }))
  return <VStack gap={8}>
    <p className="memory-muted">{tr(forgotten
      ? 'Forgotten notes stay in the history. Bring one back if you change your mind.'
      : 'What this bot believes about you, the least sure first. Confirm what holds; tell the bot what to correct.')}</p>
    <TabChoice label="Kind" selection={kind} options={[{ id: ALL, label: 'All' }, ...KINDS]} onChange={setKind} />
    <TabChoice label="Confidence" selection={confidence} options={[{ id: ALL, label: 'Any' }, ...CONFIDENCES]} onChange={setConfidence} />
    <Button kind="secondary" onClick={() => setForgotten(!forgotten)}>{forgotten ? 'Back to what I know' : 'Show forgotten'}</Button>
    {memory.beliefError ? <p role="alert" className="memory-error">{memory.beliefError}</p> : null}
    {shown.length === 0 ? <p className="memory-muted">{tr(forgotten ? 'Nothing forgotten.' : 'Nothing here yet.')}</p> : null}
    {shown.map((belief) => <BeliefRow key={`${belief.shared}:${belief.file}`} belief={belief} memory={memory} forgotten={forgotten}
      expanded={open === `${belief.shared}:${belief.file}`} toggle={() => setOpen(open === `${belief.shared}:${belief.file}` ? null : `${belief.shared}:${belief.file}`)} />)}
  </VStack>
}

function BeliefRow({ belief, memory, forgotten, expanded, toggle }: { belief: Belief; memory: MemoryStore; forgotten: boolean; expanded: boolean; toggle: () => void }) {
  const tr = useTr()
  return <div className="memory-content fade-in">
    <strong className="line-clamp memory-clamp" title={belief.description ?? belief.file}>{belief.description ?? belief.file}</strong>
    <small>{tr(kindLabel(belief.kind))} · {tr(confidenceLabel(belief.confidence))} · {tr(confirmedText(belief.confirmed))}{belief.shared ? ` · ${tr('Shared')}` : ''}</small>
    <HStack gap={8}>
      {forgotten ? null : <Button kind="secondary" onClick={() => void memory.beliefAction(belief, 'confirm')}>Still true</Button>}
      {forgotten ? null : <Button kind="secondary" onClick={() => void memory.beliefAction(belief, 'forget')}>Forget</Button>}
      <Button kind="secondary" onClick={toggle}>{expanded ? 'Hide history' : 'History'}</Button>
    </HStack>
    {expanded ? historyRows(belief).map((row) => <HStack key={row.id} gap={8}>
      <Text grow>{tr(row.label)} · {relativeTime.day(row.at, tr)}</Text>
      {row.restorable ? <Button kind="secondary" onClick={() => void memory.beliefAction(belief, 'restore', row.id)}>Bring back</Button> : null}
    </HStack>) : null}
  </div>
}
