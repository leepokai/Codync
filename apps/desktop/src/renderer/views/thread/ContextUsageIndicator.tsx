import { useCallback, useId, useRef, useState } from 'react'
import type { ContextUsage } from '@shared/context-usage'
import { ProgressRing } from '../../components/Controls'
import { AnchoredPanel } from '../../components/AnchoredPanel'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import './context-usage.css'

/** A quiet meter beside Live, with a small anchored reading instead of a modal. */
export function ContextUsageIndicator({ usage }: { usage: ContextUsage | null | undefined }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  const button = useRef<HTMLButtonElement>(null)
  const id = useId()
  const close = () => { setOpen(false); button.current?.focus() }
  const focusBubble = useCallback((node: HTMLDivElement | null) => node?.focus(), [])
  const label = usage
    ? tr('Context window: {percent}% used').replace('{percent}', String(usage.percent))
    : tr('Context usage unavailable')
  return <>
    <button ref={button} className="press context-usage" data-level={usage?.level ?? 'unknown'} title={label}
      aria-label={label} aria-haspopup="dialog" aria-expanded={open} aria-controls={open ? id : undefined}
      onClick={() => setOpen(!open)}>
      <ProgressRing value={(usage?.percent ?? 0) / 100} size={25} />
      <Text className="context-usage-label" tabular>{usage ? `${usage.percent}%` : '–'}</Text>
    </button>
    <AnchoredPanel open={open} onClose={close} anchor={button}>
      <div ref={focusBubble} id={id} role="dialog" tabIndex={-1} aria-label={tr('Context window')} className="context-usage-bubble">
        <VStack gap={6}>
          <Text size="caption" tone="secondary">{tr('Context window')}</Text>
          {usage ? <>
            <Text size="title3" weight="semibold" tabular>{usage.percent}%</Text>
            <Text size="body" tabular>{tr('{used} / {max} tokens')
              .replace('{used}', usage.usedTokens.toLocaleString()).replace('{max}', usage.maxTokens.toLocaleString())}</Text>
          </> : <Text size="callout" tone="secondary">{tr("The agent has not reported this session's context usage yet.")}</Text>}
        </VStack>
      </div>
    </AnchoredPanel>
  </>
}
