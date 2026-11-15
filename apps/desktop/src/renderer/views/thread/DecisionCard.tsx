import { useEffect, useRef, useState } from 'react'
import type { DecisionStatus, Entry } from '@shared/models'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { IconButton, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { trNow } from '../../lib/l10n.ts'
import './permission-card.css'

/** The card a bot's `request_decision` puts in the chat: its question, a numbered row per option (the suggested one
 *  marked), a last row for the user's own words, and what was picked. A press goes back as the bot's answer
 *  (`respondDecision`); own words go as an ordinary message, which the computer takes as the answer. */
export function DecisionCard({ entry, answering, respond, reply }: {
  entry: Entry
  answering: string | null
  respond: (choice: string) => void
  reply: (text: string) => void
}) {
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
      <VStack gap={12} className="permission-card decision-card">
        <HStack gap={8} align="flex-start">
          <div className="decision-card-question-box" style={{ maxHeight: height ?? undefined }} onTransitionEnd={settle}>
            <Text ref={question} as="div" size="headline" weight="semibold" tone="text" lines={expanded ? undefined : 4} className="selectable permission-card-title decision-card-question">
              {d.question ?? ''}
            </Text>
          </div>
          {expanded || clamped ? (
            <IconButton title={expanded ? 'Collapse' : 'Expand'} icon={expanded ? 'chevron.up' : 'chevron.down'} size={24} className="decision-card-more" onClick={toggle} />
          ) : null}
        </HStack>
        {pending ? (
          <VStack gap={2} className="decision-options">
            {choices.map((choice, i) => (
              <button
                key={`${i}-${choice}`}
                className={answering === null || answering === choice ? 'decision-option' : 'decision-option dimmed'}
                disabled={answering !== null}
                onClick={() => respond(choice)}
              >
                <Badge>{answering === choice ? <Spinner size={12} /> : i + 1}</Badge>
                <Text size="body" weight={suggested === i ? 'semibold' : 'regular'} tone="text" className="decision-option-label">{choice}</Text>
                {suggested === i ? <Text size="caption" tone="tertiary">{trNow('Suggested')}</Text> : null}
              </button>
            ))}
            <OwnAnswer disabled={answering !== null} reply={reply} />
          </VStack>
        ) : (
          <HStack gap={6}>
            <Icon name={status === 'answered' ? 'checkmark' : 'clock'} size={10} color="var(--secondary)" />
            <Text as="div" size="caption" weight="medium" tone="secondary" className="decision-outcome">
              {status === 'answered' && chosen ? chosen : trNow('No answer — the bot moved on')}
            </Text>
          </HStack>
        )}
      </VStack>
    </div>
  )
}

/** A row's number, or the own answer's pencil: a screen reader reads the row itself. */
function Badge({ children }: { children: React.ReactNode }) {
  return (
    <span className="decision-badge" aria-hidden>
      <Text size="caption" weight="semibold" tone="secondary" tabular>{children}</Text>
    </span>
  )
}

/** The last row: the user's own answer, sent with Return or the send button. A pencil marks it, not a number: a typed
 *  number only ever picks an option. */
function OwnAnswer({ disabled, reply }: { disabled: boolean; reply: (text: string) => void }) {
  const [text, setText] = useState('')
  const send = () => {
    if (!text.trim() || disabled) return
    reply(text.trim())
    setText('')
  }
  const prompt = trNow('Something else')
  return (
    <div className={disabled ? 'decision-option decision-own dimmed' : 'decision-option decision-own'}>
      <Badge><Icon name="pencil" size={11} /></Badge>
      <input
        value={text}
        placeholder={prompt}
        aria-label={prompt}
        disabled={disabled}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && !e.nativeEvent.isComposing) {
            e.preventDefault()
            send()
          }
        }}
      />
      {text.trim() ? <IconButton title="Send" icon="arrow.up" size={24} disabled={disabled} onClick={send} /> : null}
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
