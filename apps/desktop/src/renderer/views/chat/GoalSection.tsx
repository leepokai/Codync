import { useState } from 'react'
import type { Bot } from '@shared/models'
import { Button } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import './goal.css'

const reasons: Record<string, string> = {
  done: 'Goal complete', needsUser: 'Needs you', failed: 'Failed',
  user: 'Stopped after your message', limit: 'Goal limit reached',
  usage: 'Agent usage limit reached', stopped: 'Stopped',
}

export function GoalBanner({ bot }: { bot: Bot }) {
  const store = useStore()
  const tr = useTr()
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)
  const goal = bot.goal
  if (!goal) return null
  const stop = async (clear = false) => {
    setBusy(true)
    setError('')
    try {
      if (clear) await store.setGoal(bot.id, '', 20, 60)
      else await store.stopGoal(bot.id)
    }
    catch (e) { setError(String(e)) }
    finally { setBusy(false) }
  }
  return <HStack gap={12} className="goal-banner" aria-label={tr('Conversation goal')}>
    <VStack grow gap={4} className="goal-summary">
      <Text size="body" weight="semibold">{goal.text}</Text>
      <Text size="compactBody" tone="secondary">{goal.turns}/{goal.maxTurns} · {tr('Maximum minutes')}: {goal.maxMinutes} · {tr(goal.stopped ? reasons[goal.stopped] ?? 'Stopped' : 'Working')}</Text>
      {error ? <Text size="compactBody" tone="danger" role="alert">{error}</Text> : null}
    </VStack>
    {!goal.stopped ? <Button disabled={store.isOffline || busy} onClick={() => void stop()}>Stop goal</Button> : null}
    {goal.stopped ? <Button disabled={store.isOffline || busy} onClick={() => void stop(true)}>Clear goal</Button> : null}
  </HStack>
}

export function GoalSection({ bot }: { bot: Bot }) {
  const store = useStore()
  const tr = useTr()
  const [text, setText] = useState('')
  const [turns, setTurns] = useState(20)
  const [minutes, setMinutes] = useState(60)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  if (bot.goal === undefined) return null
  const start = async () => {
    setBusy(true)
    setError('')
    try { await store.setGoal(bot.id, text.trim(), turns, minutes) }
    catch (e) { setError(String(e)) }
    finally { setBusy(false) }
  }
  const valid = text.trim().length > 0 && Number.isInteger(turns) && turns >= 1 && turns <= 200 && Number.isInteger(minutes) && minutes >= 1 && minutes <= 1440
  return <section className="details-section goal-section">
    <Text as="h3" size="headline">{tr('Conversation goal')}</Text>
    <VStack gap={5}>
      <label htmlFor={`goal-${bot.id}`}><Text size="compactBody">{tr('Goal')}</Text></label>
      <textarea id={`goal-${bot.id}`} aria-label={tr('Goal')} maxLength={2000} value={text} onChange={(e) => setText(e.target.value)} />
    </VStack>
    <HStack gap={12} className="goal-limits">
      <VStack grow gap={5}>
        <label htmlFor={`goal-turns-${bot.id}`}><Text size="compactBody">{tr('Maximum turns')}</Text></label>
        <input id={`goal-turns-${bot.id}`} aria-label={tr('Maximum turns')} type="number" min={1} max={200} value={turns} onChange={(e) => setTurns(Number(e.target.value))} />
      </VStack>
      <VStack grow gap={5}>
        <label htmlFor={`goal-minutes-${bot.id}`}><Text size="compactBody">{tr('Maximum minutes')}</Text></label>
        <input id={`goal-minutes-${bot.id}`} aria-label={tr('Maximum minutes')} type="number" min={1} max={1440} value={minutes} onChange={(e) => setMinutes(Number(e.target.value))} />
      </VStack>
    </HStack>
    <Text as="div" size="compactBody" tone="secondary" className="goal-explanation">{tr('Continues until complete, needs you, or reaches a limit. Your next message stops continuation.')}</Text>
    <Button kind="primary" disabled={!valid || store.isOffline || busy} onClick={() => void start()}>Start goal</Button>
    {error ? <Text as="div" size="compactBody" tone="danger" role="alert">{error}</Text> : null}
  </section>
}
