import { useCallback, useEffect, useState } from 'react'
import {
  SHOWN_RUNS, SHOWN_WORKFLOWS, decodeRuns, decodeWorkflows, runFailed, runLabel,
  type N8nRun, type N8nWorkflowList,
} from '@shared/n8n-workflows'
import { CardSection, Switch } from '../../components/Controls'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { errorText, relativeDay } from './parts'

/** How often the card looks at n8n again while it shows. */
const REFRESH_MILLIS = 5_000

/**
 * The workflows the computer's n8n runs, each with a switch to publish it, and its latest runs. Only
 * mounted while n8n answers; the host does the work, the same calls the phone's section makes.
 */
export function N8nWorkflows({ disabled }: { disabled: boolean }) {
  const store = useStore()
  const tr = useTr()
  const [list, setList] = useState<N8nWorkflowList | null>(null)
  const [runs, setRuns] = useState<N8nRun[]>([])
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)

  const look = useCallback(async () => {
    const client = await store.ready()
    setList(decodeWorkflows(await client.call('n8nWorkflows', {})))
    setRuns(decodeRuns(await client.call('n8nExecutions', {})))
  }, [store])

  useEffect(() => {
    let alive = true
    const tick = () => look().then(() => alive && setError(null), (e) => alive && setError(errorText(e)))
    void tick()
    const timer = setInterval(() => void tick(), REFRESH_MILLIS)
    return () => {
      alive = false
      clearInterval(timer)
    }
  }, [look])

  const publish = async (id: string, active: boolean) => {
    setBusy(true)
    setError(null)
    try {
      const client = await store.ready()
      await client.call('n8nSetWorkflowActive', { id, active })
      await look()
    } catch (e) {
      setError(errorText(e))
    } finally {
      setBusy(false)
    }
  }

  const workflows = list?.workflows ?? []
  const hidden = workflows.length - SHOWN_WORKFLOWS
  const nameOf = (id: string | null) => workflows.find((w) => w.id === id)?.name || id || ''

  return (
    <>
      <CardSection title="Workflows" footer="Turning one on publishes it in n8n.">
        {list === null || workflows.length === 0 ? (
          <div className="settings-row">
            <Text tone="secondary">{tr(list === null ? 'Checking {name}…' : 'No workflows yet.').replace('{name}', 'n8n')}</Text>
          </div>
        ) : (
          workflows.slice(0, SHOWN_WORKFLOWS).map((w) => (
            <div className="settings-row" key={w.id}>
              <Switch on={w.active} disabled={busy || disabled} label={w.name || w.id} onChange={(on) => void publish(w.id, on)}>
                {w.name || w.id}
              </Switch>
            </div>
          ))
        )}
        {hidden > 0 || list?.more ? (
          <div className="settings-row">
            <Text size="footnote" tone="secondary">{tr('{count} more in n8n').replace('{count}', hidden > 0 ? String(hidden) : '+')}</Text>
          </div>
        ) : null}
        {error ? <Text size="footnote" tone="danger" className="fade-in">{error}</Text> : null}
      </CardSection>
      {runs.length > 0 ? (
        <CardSection title="Latest runs">
          {runs.slice(0, SHOWN_RUNS).map((run) => (
            <div className="settings-row" key={run.id}>
              <HStack gap={8} grow className="settings-stack">
                <Text grow truncate>{nameOf(run.workflowId)}</Text>
                <Text size="footnote" tone={runFailed(run.status) ? 'danger' : 'secondary'}>{tr(runLabel(run.status))}</Text>
                {run.startedAt ? <Text size="footnote" tone="secondary">{relativeDay(new Date(run.startedAt))}</Text> : null}
              </HStack>
            </div>
          ))}
        </CardSection>
      ) : null}
    </>
  )
}
