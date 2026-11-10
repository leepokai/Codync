import { useEffect, useState } from 'react'
import { Button, IconButton } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'

interface Trigger {
  type?: string
  source?: string
  event?: string
  filters?: Record<string, string>
}

interface Routine {
  id: string
  name: string
  enabled: boolean
  triggers?: Trigger[]
  trigger?: Trigger
}

const filterKey = (provider: string) => (provider === 'telegram' ? 'chat' : 'channel')

/** The routines that listen to this channel, and a short way to make one. */
export function ChannelRoutines({ provider, channel, botId }: { provider: 'telegram' | 'slack' | 'discord'; channel: string; botId: string }) {
  const tr = useTr()
  const store = useStore()
  const [routines, setRoutines] = useState<Routine[]>([])
  const [open, setOpen] = useState(false)
  const [name, setName] = useState('')
  const [instruction, setInstruction] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const matches = (routine: Routine): boolean => {
    const triggers = routine.triggers ?? (routine.trigger ? [routine.trigger] : [])
    return triggers.some((trigger) =>
      (trigger.type === 'event' || trigger.source !== undefined) &&
      trigger.source === provider &&
      (trigger.filters?.[filterKey(provider)] === channel || trigger.filters?.channel === channel || trigger.filters?.chat === channel))
  }
  const load = async () => {
    try {
      const client = await store.ready()
      const answer = await client.call<{ routines: Routine[] }>('routines', { botId })
      setRoutines((answer.routines ?? []).filter(matches))
      setError(null)
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
  }
  useEffect(() => {
    if (botId) void load()
  }, [botId, provider, channel])
  const create = async () => {
    setBusy(true)
    try {
      const client = await store.ready()
      await client.call('saveRoutine', {
        botId,
        name: name.trim(),
        instruction: instruction.trim(),
        timeoutSeconds: 600,
        schedule: {
          kind: 'event',
          eventSource: provider,
          eventName: '*',
          eventFilters: { [filterKey(provider)]: channel },
          zone: 'UTC',
        },
      })
      setName('')
      setInstruction('')
      setOpen(false)
      await load()
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason))
    }
    setBusy(false)
  }
  return (
    <VStack gap={4}>
      <HStack gap={8}>
        <Text size="caption" tone="tertiary" grow>{tr('Routines that listen to this channel')}</Text>
        <IconButton title={tr('New routine for this channel')} icon="plus.circle" size={14} onClick={() => setOpen((on) => !on)} />
      </HStack>
      {routines.map((routine) => (
        <HStack key={routine.id} gap={8}>
          <Text size="caption" grow>{routine.name}</Text>
          <Text size="caption" tone={routine.enabled ? 'secondary' : 'tertiary'}>{tr(routine.enabled ? 'On' : 'Off')}</Text>
        </HStack>
      ))}
      {!routines.length && !open ? <Text size="caption" tone="tertiary">{tr('None yet.')}</Text> : null}
      {open ? (
        <VStack gap={6}>
          <input className="field-box" value={name} placeholder={tr('Routine name')} aria-label={tr('Routine name')} onChange={(e) => setName(e.target.value)} />
          <input className="field-box" value={instruction} placeholder={tr('What it should do when this channel mentions the bot')}
            aria-label={tr('Instruction')} onChange={(e) => setInstruction(e.target.value)} />
          <HStack gap={6}>
            <Button kind="secondary" disabled={busy || !name.trim() || !instruction.trim()} onClick={() => void create()}>{tr('Create')}</Button>
            <Button kind="secondary" onClick={() => setOpen(false)}>{tr('Cancel')}</Button>
          </HStack>
        </VStack>
      ) : null}
      {error ? <Text size="caption" role="alert" tone="danger">{error}</Text> : null}
    </VStack>
  )
}
