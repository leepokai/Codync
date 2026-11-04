import { useEffect, useState } from 'react'
import { Button, CardSection, ValueRow } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { agentRows, healthLines, levelLabel, thresholdProblem } from '../../../shared/health'
import { useApp } from '../../store/context'
import { Field } from '../marketplace/pieces'
import { errorText } from './parts'

/** The computer's memory, CPU and disk, what each agent uses, and the levels that warn and stop new turns. */
export function HealthSection() {
  const app = useApp()
  const local = useModel(app.local)
  const health = local?.health ?? null
  const [warn, setWarn] = useState('')
  const [critical, setCritical] = useState('')
  const [error, setError] = useState<string | null>(null)
  useEffect(() => {
    void local?.refreshHealth()
    const timer = setInterval(() => void local?.refreshHealth(), 10_000)
    return () => clearInterval(timer)
  }, [local])
  useEffect(() => {
    if (!health) return
    setWarn(String(health.thresholds.warn))
    setCritical(String(health.thresholds.critical))
  }, [health?.thresholds.warn, health?.thresholds.critical])
  if (!health) return null
  const problem = thresholdProblem(Number(warn), Number(critical))
  const unchanged = Number(warn) === health.thresholds.warn && Number(critical) === health.thresholds.critical
  const save = async () => {
    setError(null)
    try { await local?.setHealthThresholds(Number(warn), Number(critical)) } catch (e) { setError(errorText(e)) }
  }
  return <CardSection title="Computer health" footer="At the warning level the chats say memory is getting full. At the limit Codync starts no new turns, idle agents give their memory back, and an agent whose memory keeps growing is stopped.">
    <ValueRow label="Status">
      <HStack gap={4}>
        <Icon name={health.level === 'normal' ? 'checkmark.circle.fill' : 'exclamationmark.triangle.fill'} size={13}
          color={health.level === 'normal' ? 'var(--success, #30d158)' : 'var(--warning, #ff9f0a)'} />
        <Text tone={health.level === 'normal' ? 'text' : 'attention'}>{levelLabel(health.level)}</Text>
      </HStack>
    </ValueRow>
    {healthLines(health).map((line) => <ValueRow key={line.label} label={line.label}><Text tone="secondary">{line.value}</Text></ValueRow>)}
    {agentRows(health).map((row) => <ValueRow key={row.key} label={row.name} detail="Agent"><Text tone="secondary">{row.use}</Text></ValueRow>)}
    <Field label="Warn at (% of memory)" value={warn} onChange={setWarn} placeholder="70" />
    <Field label="Stop new turns at (% of memory)" value={critical} onChange={setCritical} placeholder="85" />
    {problem && !unchanged ? <Text size="footnote" tone="danger">{problem}</Text> : null}
    {error ? <Text size="footnote" tone="danger">{error}</Text> : null}
    <Button kind="secondary" disabled={problem !== null || unchanged} onClick={() => void save()}>Save levels</Button>
  </CardSection>
}
