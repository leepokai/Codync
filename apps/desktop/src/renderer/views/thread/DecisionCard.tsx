import { useState } from 'react'
import type { Entry } from '@shared/models'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { trNow } from '../../lib/l10n.ts'
import './permission-card.css'

/** The card a bot's `request_decision` puts in the chat: its question, a button per option, the suggested one marked,
 *  and what the user picked. The press goes back as their message and as the bot's answer (`respondDecision`), so the
 *  card reads the same after it and when it is loaded again. */
export function DecisionCard({ entry, answering, respond }: { entry: Entry; answering: string | null; respond: (choice: string) => void }) {
  const d = entry.data
  const [expanded, setExpanded] = useState(false)
  const pending = d.status === 'pending'
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
        <Text as="div" size="subheadline" tone="text" lines={expanded ? undefined : 4} className="selectable permission-card-title decision-card-question">
          {d.question ?? ''}
        </Text>
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
            <Icon name={d.status === 'answered' ? 'checkmark' : 'clock'} size={10} color="var(--secondary)" />
            <Text as="div" size="caption" weight="medium" tone="secondary">
              {d.status === 'answered' && chosen ? chosen : trNow('No answer — the bot moved on')}
            </Text>
          </HStack>
        )}
        {(d.question ?? '').length > 0 ? (
          <button className="press decision-card-more" onClick={() => setExpanded((e) => !e)}>
            <Text size="caption" weight="medium" tone="secondary">{expanded ? trNow('Less') : trNow('More')}</Text>
          </button>
        ) : null}
      </VStack>
    </div>
  )
}
