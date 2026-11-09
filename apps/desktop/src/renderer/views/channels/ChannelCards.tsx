import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { Icon } from '../../components/Icon'
import { ServiceLogo } from '../marketplace/pieces'
import type { ChannelProviderEntry } from './channel-catalog'
import './channels.css'

const PROVIDER_DOMAIN = { whatsapp: 'whatsapp.com', telegram: 'telegram.org', slack: 'slack.com', discord: 'discord.com' } as const

/** One messaging channel as a card: its logo, what it is and its state at a glance. */
export function ChannelCard({ entry, onOpen }: { entry: ChannelProviderEntry; onOpen: () => void }) {
  const tr = useTr()
  return (
    <button type="button" className="channel-card" onClick={onOpen}>
      <ServiceLogo website={PROVIDER_DOMAIN[entry.id]} name={entry.name} size={44} />
      <VStack gap={2} grow className="channel-card-text">
        <HStack gap={6}>
          <Text weight="semibold">{entry.name}</Text>
          <Text size="caption" tone={entry.needsAttention ? 'danger' : entry.connected ? 'secondary' : 'tertiary'}>
            {tr(entry.connected ? (entry.needsAttention ? 'Needs attention' : 'Connected') : 'Not connected')}
          </Text>
          <span className={`channel-dot ${entry.needsAttention ? 'error' : entry.connected ? 'ok' : 'off'}`} aria-hidden />
        </HStack>
        <Text size="caption" tone="secondary">{tr(entry.summary)}</Text>
      </VStack>
      <Icon name="chevron.right" size={10} weight="semibold" color="var(--tertiary)" />
    </button>
  )
}

/** The four channels, in the order the Marketplace lists them. */
export function ChannelCards({ entries, onOpen }: { entries: ChannelProviderEntry[]; onOpen: (entry: ChannelProviderEntry) => void }) {
  return (
    <div className="channel-grid">
      {entries.map((entry) => <ChannelCard key={entry.id} entry={entry} onOpen={() => onOpen(entry)} />)}
    </div>
  )
}
