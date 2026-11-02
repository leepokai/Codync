import { useEffect, useRef, useState } from 'react'
import { isGroup } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { ChipChoice, ChoicePicker, IconButton, SegmentedChoice } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Dialog } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { fetchWorkflow, rotateWorkflowWebhook } from './api'
import { NOTE_TONES, SHAPES, fieldString, type Node } from './workflow'
import { RoutineScheduleFields } from '../routines/RoutineScheduleFields'
import { errorText, type RoutineScheduleDraft, type RoutineSchedulePreview } from '../routines/routine'
import { currentZone } from '../routines/time-zones'

export function WorkflowFields({ node, readOnly, onChange }: { node: Node; readOnly: boolean; onChange: (node: Node) => void }) {
  const patch = (fields: Node['fields'], schedule?: Node['schedule']) => {
    if (readOnly) return
    onChange({ ...node, fields: { ...node.fields, ...fields }, ...(schedule !== undefined ? { schedule } : {}) })
  }
  switch (node.kind) {
    case 'trigger':
      return <TriggerFields node={node} readOnly={readOnly} patch={patch} onChange={onChange} />
    case 'prompt':
      return <Area label="Prompt" placeholder="Write a prompt" value={fieldString(node.fields, 'text')} readOnly={readOnly} onChange={(text) => patch({ text })} />
    case 'agent':
      return <AgentFields node={node} readOnly={readOnly} patch={patch} />
    case 'condition':
      return <ConditionFields node={node} readOnly={readOnly} patch={patch} />
    case 'approval':
      return <Area label="Question" value={fieldString(node.fields, 'question')} placeholder="Approve this step?" readOnly={readOnly} onChange={(question) => patch({ question })} />
    case 'output':
      return <OutputFields node={node} patch={patch} />
    case 'text':
      return <Area label="Text" value={fieldString(node.fields, 'text')} readOnly={readOnly} onChange={(text) => patch({ text })} />
    case 'note':
      return (
        <VStack gap={8}>
          <Area label="Note" value={fieldString(node.fields, 'text')} readOnly={readOnly} onChange={(text) => patch({ text })} />
          <Tones value={fieldString(node.fields, 'tone') || 'yellow'} readOnly={readOnly} onChange={(tone) => patch({ tone })} />
        </VStack>
      )
    case 'shape':
      return (
        <VStack gap={8}>
          <ChoicePicker selection={fieldString(node.fields, 'shape') || 'rectangle'} options={SHAPES.map((id) => ({ id, label: id[0]!.toUpperCase() + id.slice(1) }))} onChange={(shape) => patch({ shape })} />
          <Area label="Text" value={fieldString(node.fields, 'text')} readOnly={readOnly} onChange={(text) => patch({ text })} />
          <Tones value={fieldString(node.fields, 'fill') || 'gray'} readOnly={readOnly} onChange={(fill) => patch({ fill })} />
        </VStack>
      )
    default:
      return <Text size="caption" tone="secondary">Update Codync on this computer to edit this workflow.</Text>
  }
}

function Area({ label, value, placeholder, rows = 3, readOnly, onChange }: { label: string; value: string; placeholder?: string; rows?: number; readOnly: boolean; onChange: (value: string) => void }) {
  const tr = useTr()
  return <textarea className="workflow-input" aria-label={tr(label)} rows={rows} value={value} placeholder={placeholder ? tr(placeholder) : undefined} readOnly={readOnly} onChange={(event) => onChange(event.target.value)} />
}

function Tones({ value, readOnly, onChange }: { value: string; readOnly: boolean; onChange: (tone: string) => void }) {
  return (
    <HStack gap={6}>
      {NOTE_TONES.map((tone) => (
        <button key={tone} type="button" className={`workflow-swatch tone-${tone} ${value === tone ? 'on' : ''}`} aria-label={tone} aria-pressed={value === tone} disabled={readOnly} onClick={() => onChange(tone)} />
      ))}
    </HStack>
  )
}

function TriggerFields({ node, readOnly, patch, onChange }: { node: Node; readOnly: boolean; patch: (fields: Node['fields'], schedule?: Node['schedule']) => void; onChange: (node: Node) => void }) {
  const kind = fieldString(node.fields, 'trigger') || 'manual'
  return (
    <VStack gap={8}>
      <SegmentedChoice
        selection={kind}
        options={[{ id: 'manual', label: 'Manual' }, { id: 'schedule', label: 'Schedule' }, { id: 'webhook', label: 'Webhook' }]}
        onChange={(trigger) => {
          if (readOnly) return
          if (trigger === 'schedule') onChange({ ...node, fields: { ...node.fields, trigger } })
          else patch({ trigger, timezone: '' }, null)
        }}
      />
      {kind === 'schedule' ? <ScheduleFields node={node} readOnly={readOnly} onChange={onChange} /> : null}
      {kind === 'webhook' ? <WebhookFields node={node} /> : null}
    </VStack>
  )
}

function ScheduleFields({ node, readOnly, onChange }: { node: Node; readOnly: boolean; onChange: (node: Node) => void }) {
  const store = useStore()
  const [draft, setDraft] = useState<RoutineScheduleDraft | null>(null)
  const [preview, setPreview] = useState<RoutineSchedulePreview | null>(null)
  const [error, setError] = useState<string | null>(null)
  const request = useRef(0)
  const nodeRef = useRef(node)
  nodeRef.current = node

  useEffect(() => {
    let cancel = false
    const schedule = node.schedule
    void (async () => {
      try {
        const client = await store.ready()
        const result = await client.call<RoutineSchedulePreview>('routineSchedule', schedule
          ? { triggers: [{ type: 'cron', expression: schedule.expression, timeZone: schedule.timeZone }], timeZone: schedule.timeZone }
          : { triggers: [], timeZone: currentZone(Intl.DateTimeFormat().resolvedOptions().timeZone) })
        if (!cancel) {
          setDraft({ ...result.draft, kind: 'cron' })
          setPreview(result)
        }
      } catch (cause) {
        if (!cancel) setError(errorText(cause))
      }
    })()
    return () => { cancel = true }
  }, [node.id])

  const update = (change: Partial<RoutineScheduleDraft>) => {
    if (!draft || readOnly) return
    const next = { ...draft, ...change, kind: 'cron' }
    setDraft(next)
    const token = ++request.current
    void (async () => {
      try {
        const result = await (await store.ready()).call<RoutineSchedulePreview>('routineSchedule', { draft: next })
        if (token !== request.current) return
        const current = nodeRef.current
        if (current.fields.trigger !== 'schedule') return
        setPreview(result)
        setError(null)
        const cron = result.triggers.find((trigger) => trigger.type === 'cron')
        if (!cron?.expression || !cron.timeZone) return
        onChange({ ...current, fields: { ...current.fields, trigger: 'schedule', timezone: cron.timeZone }, schedule: { type: 'cron', expression: cron.expression, timeZone: cron.timeZone } })
      } catch (cause) {
        if (token === request.current) setError(errorText(cause))
      }
    })()
  }

  if (!draft) return error ? <Text size="caption" tone="danger">{error}</Text> : null
  return <RoutineScheduleFields schedule={draft} update={update} preview={preview} error={error} />
}

function WebhookFields({ node }: { node: Node }) {
  const tr = useTr()
  const store = useStore()
  const [url, setUrl] = useState('')
  const [secret, setSecret] = useState<string | null>(null)
  const [shown, setShown] = useState(false)
  const [confirm, setConfirm] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const workflowId = store.workflowSession?.id

  useEffect(() => {
    const hook = store.workflowDetail?.webhooks?.find((item) => item.nodeId === node.id)
    setUrl(hook?.url || hook?.localUrl || '')
    setSecret(hook?.secret ?? null)
  }, [store.workflowDetail, node.id])

  const reveal = async () => {
    if (!workflowId) return
    try {
      const { workflow } = await fetchWorkflow(await store.ready(), workflowId, true)
      const hook = workflow.webhooks?.find((item) => item.nodeId === node.id)
      setSecret(hook?.secret ?? null)
      setShown(true)
      setError(null)
    } catch (cause) {
      setError(errorText(cause))
    }
  }

  const rotate = async () => {
    if (!workflowId) return
    setConfirm(false)
    try {
      const { secret: next } = await rotateWorkflowWebhook(await store.ready(), workflowId, node.id)
      setSecret(next)
      setShown(true)
      setError(null)
    } catch (cause) {
      setError(errorText(cause))
    }
  }

  return (
    <VStack gap={6}>
      {error ? <Text size="caption" tone="danger">{error}</Text> : null}
      <Text size="caption" mono lines={2}>{url || tr('Saving…')}</Text>
      <HStack gap={4}>
        <Text size="caption" mono grow>{shown && secret ? secret : '••••••••••••••••'}</Text>
        <IconButton title={shown ? 'Hide key' : 'Show key'} icon={shown ? 'eye.slash' : 'eye'} onClick={() => (shown ? setShown(false) : void reveal())} />
        <IconButton title="Copy" icon="doc.on.doc" onClick={() => secret && window.codync.app.copy(secret)} />
        <IconButton title="Replace key" icon="arrow.triangle.2.circlepath" onClick={() => setConfirm(true)} />
      </HStack>
      <Dialog
        open={confirm}
        title={tr('Replace the key?')}
        message={tr('Senders using the current key stop working until they get the new one.')}
        actions={[{ title: 'Replace key', destructive: true, action: () => void rotate() }]}
        onClose={() => setConfirm(false)}
      />
    </VStack>
  )
}

function AgentFields({ node, readOnly, patch }: { node: Node; readOnly: boolean; patch: (fields: Node['fields']) => void }) {
  const store = useStore()
  const bots = [...store.bots.values()].filter((bot) => !isGroup(bot))
  const botId = fieldString(node.fields, 'botId')
  const bot = bots.find((item) => item.id === botId)
  const model = bot?.model || (bot ? store.backendName(bot.backend) : '')
  return (
    <VStack gap={8}>
      <Area label="Task" rows={2} placeholder="Tell the assistant what to do" value={fieldString(node.fields, 'task')} readOnly={readOnly} onChange={(task) => patch({ task })} />
      <HStack className="workflow-agent-row" gap={6} shrink>
        <ChoicePicker
          selection={botId}
          options={bots.map((item) => ({ id: item.id, label: item.name, leading: <BotAvatar bot={item} size={16} animated={false} /> }))}
          onChange={(id) => patch({ botId: id })}
        />
        {model ? <Text className="workflow-model" size="caption" truncate title={model}>{model}</Text> : null}
      </HStack>
      {!bot ? <Text size="caption" tone="secondary">Choose a bot</Text> : null}
    </VStack>
  )
}

function ConditionFields({ node, readOnly, patch }: { node: Node; readOnly: boolean; patch: (fields: Node['fields']) => void }) {
  const op = fieldString(node.fields, 'op') || 'contains'
  return (
    <VStack gap={8}>
      <ChipChoice
        label="Match"
        selection={op}
        options={[
          { id: 'contains', label: 'Contains' },
          { id: 'equals', label: 'Equals' },
          { id: 'regex', label: 'Regex' },
          { id: 'empty', label: 'Empty' },
        ]}
        onChange={(next) => patch({ op: next })}
      />
      {op === 'empty' ? null : <Area label="Value" value={fieldString(node.fields, 'value')} readOnly={readOnly} onChange={(value) => patch({ value })} />}
    </VStack>
  )
}

function OutputFields({ node, patch }: { node: Node; patch: (fields: Node['fields']) => void }) {
  const store = useStore()
  const target = fieldString(node.fields, 'target') || 'notify'
  const bots = [...store.bots.values()].filter((bot) => !isGroup(bot))
  return (
    <VStack gap={8}>
      <ChoicePicker
        selection={target}
        fits
        options={[
          { id: 'notify', label: 'Notification', leading: <Icon name="bell" size={14} /> },
          { id: 'chat', label: 'Chat', leading: <Icon name="bubble.left" size={14} /> },
        ]}
        onChange={(next) => patch({ target: next })}
      />
      {target === 'chat' ? (
        <ChoicePicker selection={fieldString(node.fields, 'botId')} options={bots.map((item) => ({ id: item.id, label: item.name }))} onChange={(botId) => patch({ botId })} />
      ) : null}
    </VStack>
  )
}
