import { useState } from 'react'
import type { WhatsAppState } from '../../../shared/models'
import type { ChannelsStore } from '../../store/channels-store'
import { Button, IconButton } from '../../components/Controls'
import { Dialog, ModalHeader, SheetPage } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { FormSheet, ServiceLogo } from '../marketplace/pieces'
import { ChannelIconPicker } from '../settings/ChannelIconPicker'
import { ChannelMemory } from '../settings/ChannelMemory'
import { DiscordChannel } from '../settings/DiscordChannel'
import { AddTelegram } from '../settings/ChannelsSection'
import type { ChannelItem, ChannelProviderEntry } from './channel-catalog'

const PROVIDER_DOMAIN = { whatsapp: 'whatsapp.com', telegram: 'telegram.org', slack: 'slack.com', discord: 'discord.com' } as const

/** One linked number, bot or group channel: its state, who may write, and what it knows. */
function ItemCard({ entry, item, store, removing, onRemove }: {
  entry: ChannelProviderEntry
  item: ChannelItem
  store: ChannelsStore
  removing: boolean
  onRemove: () => void
}) {
  const tr = useTr()
  const telegram = entry.id === 'telegram'
  return (
    <section className="channel-item">
      <HStack gap={8}>
        <div className="channel-item-logo">
          <ServiceLogo website={PROVIDER_DOMAIN[entry.id]} name={entry.name} size={30} />
        </div>
        <VStack gap={2} grow>
          <Text weight="semibold">{item.label}</Text>
          <Text size="caption" tone="secondary">{item.subtitle}</Text>
        </VStack>
        <span className={`channel-dot ${item.state}`} aria-hidden />
        <Text size="caption" tone={item.state === 'error' ? 'danger' : 'secondary'}>{tr(item.stateText)}</Text>
      </HStack>
      {item.botName ? <Text size="caption" tone="tertiary">{tr('Answers with {bot}').replace('{bot}', item.botName)}</Text> : null}
      {item.people.length ? (
        <VStack gap={2}>
          <Text size="caption" tone="tertiary">{tr('People allowed')}</Text>
          {item.people.map((person) => (
            <HStack key={person.id} gap={8}>
              <Text size="caption" grow>{person.label}</Text>
              <IconButton title={tr('Remove')} icon="person.badge.minus" size={14} disabled={store.busy}
                onClick={() => void (telegram ? store.setAllowed(item.channel, Number(person.id), false) : Promise.resolve())} />
            </HStack>
          ))}
        </VStack>
      ) : null}
      {telegram ? (
        <>
          <ChannelIconPicker value={item.icon} save={(icon) => store.setIcon('telegram', item.channel, icon)} />
          <ChannelMemory value={item.memory} save={(memory) => store.setMemory('telegram', item.channel, memory)} />
          <HStack gap={6}>
            <Button kind="secondary" disabled={store.busy} onClick={() => void store.pair(item.channel)}>{tr('Pair someone')}</Button>
            <Button kind="secondary" disabled={store.busy || removing} onClick={onRemove}>{tr('Remove bot')}</Button>
          </HStack>
        </>
      ) : null}
      {entry.id === 'slack' || entry.id === 'discord' ? (
        <>
          <ChannelIconPicker value={item.icon} save={(icon) => store.setIcon(entry.id === 'discord' ? 'discord' : 'slack', item.channel, icon)} />
          <ChannelMemory value={item.memory} save={(memory) => store.setMemory(entry.id === 'discord' ? 'discord' : 'slack', item.channel, memory)} />
        </>
      ) : null}
      {entry.id === 'whatsapp' ? (
        <Text size="caption" tone="tertiary">{tr('Manage the link, its bot and the chats allowed in the WhatsApp window.')}</Text>
      ) : null}
      <Text size="caption" tone="tertiary">{item.botId ? tr('Bot assigned on this computer') : tr('No bot assigned yet')}</Text>
    </section>
  )
}

/**
 * A messaging channel's page: what it is, its state, every linked number, bot or group channel,
 * who may write to each, and what each one knows. Nothing here holds a token.
 */
export function ChannelSheet({ entry, store, whatsapp, bots, onConnect, onClose }: {
  entry: ChannelProviderEntry
  store: ChannelsStore
  whatsapp: WhatsAppState | null
  bots: { id: string; label: string }[]
  onConnect: () => void
  onClose: () => void
}) {
  const tr = useTr()
  const [removing, setRemoving] = useState<string | null>(null)
  const [adding, setAdding] = useState(false)
  const canPair = entry.id === 'whatsapp'
  return (
    <FormSheet open onClose={onClose} width={640} height={720}>
      <SheetPage>
        <ModalHeader
          title={entry.name}
          subtitle={tr(entry.description)}
          icon={<ServiceLogo website={PROVIDER_DOMAIN[entry.id]} name={entry.name} size={38} />}
          onClose={onClose}
          closeTitle={tr('Close')}
        />
        <VStack gap={14} className="channel-sheet">
          <HStack gap={8}>
            <span className={`channel-dot ${entry.needsAttention ? 'error' : entry.connected ? 'ok' : 'off'}`} aria-hidden />
            <Text size="caption" tone="secondary" grow>{entry.summary}</Text>
            <Text size="caption" tone="tertiary">{tr(entry.connected ? 'Connected' : 'Not connected')}</Text>
          </HStack>

          {entry.items.map((item) => (
            <ItemCard key={item.key} entry={entry} item={item} store={store}
              removing={removing === item.channel} onRemove={() => setRemoving(item.channel)} />
          ))}

          {entry.id === 'telegram' ? (
            adding ? (
              <section className="channel-item">
                <AddTelegram model={store} bots={bots} />
                <Button kind="secondary" onClick={() => setAdding(false)}>{tr('Done')}</Button>
              </section>
            ) : (
              <Button kind="secondary" disabled={store.busy} onClick={() => setAdding(true)}>{tr('Add Telegram bot')}</Button>
            )
          ) : null}

          {entry.id === 'discord' ? (
            <section className="channel-item">
              <DiscordChannel status={store.discord} model={store} bots={bots} />
            </section>
          ) : null}

          {canPair ? <Button disabled={store.busy} onClick={onConnect}>{tr(entry.connected ? 'Manage the link' : 'Link a number')}</Button> : null}
          {entry.id === 'slack' ? (
            <Text size="caption" tone="tertiary">
              {tr('Slack is connected on this computer with the host’s Slack setup; the signing secret and app token never reach the app.')}
            </Text>
          ) : null}
          {store.error ? <Text size="caption" role="alert" tone="danger">{store.error}</Text> : null}
          {entry.id === 'slack' && store.slack?.configured ? (
            <Button kind="secondary" disabled={store.busy} onClick={() => void store.disconnectSlack()}>{tr('Disconnect Slack')}</Button>
          ) : null}
          {entry.id === 'discord' && store.discord?.configured ? (
            <Button kind="secondary" disabled={store.busy} onClick={() => void store.disconnectDiscord()}>{tr('Disconnect Discord')}</Button>
          ) : null}
          {whatsapp?.error ? <Text size="caption" role="alert" tone="danger">{whatsapp.error}</Text> : null}
        </VStack>
        <Dialog
          open={removing !== null}
          title={tr('Remove this Telegram bot?')}
          message={tr('Its token is deleted from this computer and the people paired to it can no longer write to your bot.')}
          actions={[{ title: tr('Remove'), action: () => { const name = removing; setRemoving(null); if (name) void store.removeTelegram(name) } }]}
          onClose={() => setRemoving(null)}
        />
      </SheetPage>
    </FormSheet>
  )
}
