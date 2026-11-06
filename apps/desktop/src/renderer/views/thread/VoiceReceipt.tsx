import { useEffect, useMemo, useState } from 'react'
import type { Entry } from '@shared/models'
import { Button, Reveal } from '../../components/Controls'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import './voice-receipt.css'
import { useModel } from '../../lib/observable'
import { useStore } from '../../store/context'
import { VoiceDecisionsStore } from '../../store/voice-decisions-store'
import { Field, ErrorText } from '../marketplace/pieces'

export function VoiceReceipt({ entry }: { entry: Entry }) {
  const [expanded, setExpanded] = useState(false)
  const store = useStore()
  const model = useModel(useMemo(() => new VoiceDecisionsStore(() => store.ready(), entry.botId, entry.id), [store, entry.botId, entry.id]))
  useEffect(() => { if (expanded) void model.load(); return () => model.dispose() }, [model, expanded])
  const seconds = entry.data.callSeconds ?? 0
  return <VStack gap={8}>
    <Text size="footnote" tone="secondary">Voice chat · {Math.floor(seconds / 60)}:{String(seconds % 60).padStart(2, '0')}</Text>
    <Button kind="secondary" aria-expanded={expanded} onClick={() => setExpanded(!expanded)}>{expanded ? 'Hide call transcript' : 'Show call transcript'}</Button>
    <Reveal show={expanded}><VStack gap={8}>
      <Text as="div" size="footnote" className="selectable voice-receipt-text">{entry.data.voiceTranscriptText ?? 'No transcript was captured. Write only decisions you remember and personally confirm.'}</Text>
      <Text size="footnote" tone="secondary">Write only the decisions you personally confirm. Confirming saves an immutable receipt and executes no actions.</Text>
      {model.confirmed !== null ? <Text className="selectable voice-receipt-text">Confirmed decisions: {model.confirmed}</Text> : <>
        <Field label="Decisions to confirm" value={model.summary} onChange={(text) => model.setSummary(text)} lines={[2, 8]} />
        <Button disabled={model.loading || model.busy || !model.summary.trim() || model.summary.length > 4000} onClick={() => void model.confirm()}>Confirm these decisions</Button>
      </>}
      {model.error ? <ErrorText text={model.error} /> : null}
    </VStack></Reveal>
  </VStack>
}
