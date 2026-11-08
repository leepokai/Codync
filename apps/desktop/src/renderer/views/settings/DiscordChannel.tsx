import { useState, type ReactNode } from 'react'
import { Button, ChoicePicker } from '../../components/Controls'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { discordIdOk, discordStateText, discordTokenOk, type DiscordStatus } from '../../../shared/channels'
import { useTr } from '../../lib/use-language'
import { Field } from '../marketplace/pieces'
import type { ChannelsStore } from '../../store/channels-store'
import { ChannelMemory } from './ChannelMemory'

interface BindingDraft {
  channelId: string
  botId: string
  allowed: string
}

/** Discord is set up on the computer: the bot token stays there. */
export function DiscordChannel({ status, model, bots }: {
  status: DiscordStatus | null
  model: ChannelsStore
  bots: { id: string; label: string }[]
}) {
  const tr = useTr()
  const [editing, setEditing] = useState(false)
  const [guildId, setGuildId] = useState('')
  const [botUserId, setBotUserId] = useState('')
  const [token, setToken] = useState('')
  const [bindings, setBindings] = useState<BindingDraft[]>([{ channelId: '', botId: '', allowed: '' }])
  const update = (index: number, change: Partial<BindingDraft>) =>
    setBindings((rows) => rows.map((row, i) => (i === index ? { ...row, ...change } : row)))
  const clean = bindings
    .map((row) => ({ channelId: row.channelId.trim(), botId: row.botId || bots[0]?.id || '', allowedUsers: row.allowed.split(/[\s,]+/).filter(Boolean) }))
    .filter((row) => row.channelId || row.allowedUsers.length > 0)
  const ready = discordIdOk(guildId.trim()) && discordIdOk(botUserId.trim()) && discordTokenOk(token.trim())
    && clean.length > 0
    && clean.every((row) => discordIdOk(row.channelId) && row.botId && row.allowedUsers.length > 0 && row.allowedUsers.every(discordIdOk))
  const save = async () => {
    const ok = await model.setDiscord({ guildId: guildId.trim(), botUserId: botUserId.trim(), botToken: token.trim(), bindings: clean })
    if (ok) { setEditing(false); setToken('') }
  }

  if (status?.configured && !editing) {
    return (
      <VStack gap={4} className="settings-stack">
        <Text>{tr('Discord')} · {tr(discordStateText(status.state))}</Text>
        <Text size="caption" tone="tertiary">
          {tr('Server')} {status.guildId} · {status.bindings?.length ?? 0} {tr('channels bound')}
        </Text>
        {status.bindings?.map((binding) => (
          <VStack key={binding.channelId} gap={4} className="settings-stack">
            <Text size="caption" tone="tertiary">
              {binding.channelId} → {bots.find((b) => b.id === binding.botId)?.label ?? tr('Unknown bot')} · {binding.allowedUsers.length} {tr('people allowed')}
            </Text>
            <ChannelMemory value={binding.memory ?? ''} save={(memory) => model.setMemory('discord', binding.channelId, memory)} />
          </VStack>
        ))}
        <HStack gap={8}>
          <Button kind="secondary" disabled={model.busy} onClick={() => setEditing(true)}>{tr('Change Discord settings')}</Button>
          <Button kind="secondary" disabled={model.busy} onClick={() => void model.disconnectDiscord()}>{tr('Disconnect Discord')}</Button>
        </HStack>
      </VStack>
    )
  }

  return (
    <VStack gap={6} className="settings-stack">
      <Text size="caption" tone="tertiary">
        {tr('Make a Discord app, add its bot to the server, and paste the bot token here. In the app, turn on the Message Content intent.')}
      </Text>
      <Named label="Server id"><Field value={guildId} onChange={setGuildId} placeholder="123456789012345678" /></Named>
      <Named label="Bot user id"><Field value={botUserId} onChange={setBotUserId} placeholder="123456789012345678" /></Named>
      <Named label="Bot token"><Field secret value={token} onChange={setToken} placeholder="The token from the Bot tab" /></Named>
      {bindings.map((row, index) => (
        <VStack key={index} gap={4} className="settings-stack">
          <Named label="Channel id"><Field value={row.channelId} onChange={(value) => update(index, { channelId: value })} placeholder="123456789012345678" /></Named>
          <ChoicePicker
            selection={row.botId || bots[0]?.id || ''}
            options={bots.map((b) => ({ id: b.id, label: b.label }))}
            onChange={(botId) => update(index, { botId })}
          />
          <Named label="Allowed user ids">
            <Field value={row.allowed} onChange={(allowed) => update(index, { allowed })} placeholder="Separated by spaces or commas" />
          </Named>
          {bindings.length > 1 ? (
            <Button kind="secondary" onClick={() => setBindings((rows) => rows.filter((_, i) => i !== index))}>{tr('Remove channel')}</Button>
          ) : null}
        </VStack>
      ))}
      <HStack gap={8}>
        <Button kind="secondary" onClick={() => setBindings((rows) => [...rows, { channelId: '', botId: '', allowed: '' }])}>{tr('Add channel')}</Button>
        <Button disabled={!ready || model.busy} onClick={() => void save()}>{model.busy ? tr('Checking…') : tr('Connect Discord')}</Button>
        {status?.configured ? <Button kind="secondary" disabled={model.busy} onClick={() => setEditing(false)}>{tr('Cancel')}</Button> : null}
      </HStack>
    </VStack>
  )
}

function Named({ label, children }: { label: string; children: ReactNode }) {
  const tr = useTr()
  return <VStack gap={3}><Text size="caption" tone="secondary">{tr(label)}</Text>{children}</VStack>
}
