import { useEffect, useState } from 'react'
import { needsInput, type Bot, type Entry } from '@shared/models'
import { Reveal } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { prefs, usePref } from '../../lib/prefs'
import { useTr } from '../../lib/use-language'
import { useFollowEnd } from './follow-end'
import { TraceRow } from './TraceView'

/** The bot's current activity and elapsed time; with thinking details on (Settings), a click unfolds the turn's steps. */
export function WorkingIndicator({ bot, steps }: { bot: Bot; steps: Entry[] }) {
  const tr = useTr()
  const [details] = usePref(prefs.thinkingDetails)
  const waiting = needsInput(bot)
  const [expanded, setExpanded] = useState(false)
  const open = details && expanded
  const status = (
    <HStack as="span" gap={8}>
      <ThinkingOrb state={waiting ? 'listening' : 'working'} size={16} color={waiting ? 'var(--warning)' : 'var(--secondary)'} />
      <Text size="subheadline" tone={waiting ? 'attention' : 'secondary'} truncate grow>{tr(bot.activity || 'Working…')}</Text>
      {bot.startedAt ? <Timer since={bot.startedAt} /> : null}
      {details ? (
        <span className={`disclosure-chevron${open ? ' open' : ''}`}>
          <Icon name="chevron.right" size={10} weight="semibold" />
        </span>
      ) : null}
    </HStack>
  )
  return (
    <VStack align="flex-start" className="agent-inset">
      <div className={open ? 'working expanded' : 'working'}>
        {details ? (
          <button className="working-toggle" onClick={() => setExpanded((x) => !x)} aria-expanded={open} title={tr(open ? 'Hide steps' : 'Show steps')}>
            {status}
          </button>
        ) : (
          status
        )}
        <Reveal show={open}>
          <WorkingSteps steps={steps} />
        </Reveal>
      </div>
    </VStack>
  )
}

/** The turn's thinking, narration, tool calls and plan in a bounded box that follows new steps until the reader scrolls
 * up. A turn can have none yet: Claude's thinking text arrives as its thinking ends. */
function WorkingSteps({ steps }: { steps: Entry[] }) {
  const tr = useTr()
  const follow = useFollowEnd()
  return (
    <div ref={follow.ref} className="working-steps" onScroll={follow.onScroll}>
      <VStack gap={10}>
        {steps.length === 0 ? <Text tone="tertiary">{tr('Nothing yet.')}</Text> : null}
        {steps.map((e) => <TraceRow key={e.id} entry={e} unfolded />)}
      </VStack>
    </div>
  )
}

/** `Text(date, style: .timer)`: 0:42, 12:05, 1:02:33. */
function Timer({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [])
  const s = Math.max(0, Math.floor((now - since) / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = String(s % 60).padStart(2, '0')
  return <Text size="footnote" tone="tertiary" tabular>{h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`}</Text>
}
