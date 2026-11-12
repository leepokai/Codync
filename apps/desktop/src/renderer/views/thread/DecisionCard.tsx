import { useEffect, useRef, useState } from 'react'
import type { DecisionStatus, Entry } from '@shared/models'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { trNow } from '../../lib/l10n.ts'
import './permission-card.css'

/** The card a bot's `request_decision` puts in the chat: its question, a button per option, the suggested one marked,
 *  and what the user picked. The press goes back as their message and as the bot's answer (`respondDecision`), so the
 *  card reads the same after it and when it is loaded again. */
export function DecisionCard({ entry, answering, respond }: { entry: Entry; answering: string | null; respond: (choice: string) => void }) {
  const d = entry.data
  const status = d.status as DecisionStatus | undefined
  const question = useRef<HTMLElement>(null)
  const { expanded, height, toggle, settle } = useExpand(question)
  const clamped = useClamped(question, expanded)
  const pending = status === 'pending'
  const choices = d.choices ?? []
  const chosen = d.selected ?? null
  const suggested = d.recommended ?? null
  return (
    <div className="agent-inset">
      <VStack gap={10} className="permission-card decision-card">
        <HStack gap={8}>
          <Icon name="questionmark.bubble" size={12} color="var(--secondary)" />
          <Text size="headline" tone="text">{trNow('Needs your choice')}</Text>
          {pending ? <span className="permission-card-dot" /> : null}
        </HStack>
        <div className="decision-card-question-box" style={{ maxHeight: height ?? undefined }} onTransitionEnd={settle}>
          <Text ref={question} as="div" size="subheadline" tone="text" lines={expanded ? undefined : 4} className="selectable permission-card-title decision-card-question">
            {d.question ?? ''}
          </Text>
        </div>
        {pending ? (
          <div className="permission-card-options">
            {choices.map((choice, i) => (
              <div key={`${i}-${choice}`}>
                {i > 0 ? <div className="thread-hairline" /> : null}
                <button
                  className={answering === null || answering === choice ? 'permission-option decision-option' : 'permission-option decision-option dimmed'}
                  disabled={answering !== null}
                  onClick={() => respond(choice)}
                >
                  <Text size="body" weight={suggested === i ? 'semibold' : 'regular'} tone="text" className="permission-option-label">{choice}</Text>
                  {suggested === i ? <Text size="caption" tone="tertiary">{trNow('Suggested')}</Text> : null}
                  {answering === choice ? <Spinner size={14} /> : null}
                </button>
              </div>
            ))}
          </div>
        ) : (
          <HStack gap={6}>
            <Icon name={status === 'answered' ? 'checkmark' : 'clock'} size={10} color="var(--secondary)" />
            <Text as="div" size="caption" weight="medium" tone="secondary">
              {status === 'answered' && chosen ? chosen : trNow('No answer — the bot moved on')}
            </Text>
          </HStack>
        )}
        {expanded || clamped ? (
          <IconButton title={expanded ? 'Collapse' : 'Expand'} icon={expanded ? 'chevron.up' : 'chevron.down'} size={24} className="decision-card-more" onClick={toggle} />
        ) : null}
      </VStack>
    </div>
  )
}

/** Expands and collapses the clamped question with an animated height (`--layout`): the box's max-height runs from the
 *  height it has to the height it will have, and is lifted once it settles so the text can reflow. */
function useExpand(ref: React.RefObject<HTMLElement | null>) {
  const [expanded, setExpanded] = useState(false)
  const [collapsing, setCollapsing] = useState(false)
  const [height, setHeight] = useState<number | null>(null)
  const toggle = () => {
    const el = ref.current
    if (!el) return
    // Reduced motion (`--layout: 0s`) fires no transitionend: switch straight away.
    if (el.parentElement && getComputedStyle(el.parentElement).transitionDuration === '0s') {
      setExpanded(!expanded)
      return
    }
    const from = el.getBoundingClientRect().height
    if (!expanded) {
      setHeight(from)
      setExpanded(true)
      requestAnimationFrame(() => setHeight(el.scrollHeight))
    } else {
      setHeight(from)
      setCollapsing(true)
      requestAnimationFrame(() => setHeight(Number.parseFloat(getComputedStyle(el).lineHeight) * 4))
    }
  }
  const settle = () => {
    if (collapsing) {
      setCollapsing(false)
      setExpanded(false)
    }
    setHeight(null)
  }
  return { expanded, height, toggle, settle }
}

/** Whether the clamped question hides lines, so the expand button only shows when there is more to read. */
function useClamped(ref: React.RefObject<HTMLElement | null>, expanded: boolean) {
  const [clamped, setClamped] = useState(false)
  useEffect(() => {
    const el = ref.current
    if (!el || expanded) return
    const observer = new ResizeObserver(() => setClamped(el.scrollHeight > el.clientHeight + 1))
    observer.observe(el)
    return () => observer.disconnect()
  }, [ref, expanded])
  return clamped
}
