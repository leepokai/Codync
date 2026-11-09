import type { WhatsAppState } from '../../../shared/models'
import type { DiscordStatus, SlackStatus, TelegramBotStatus } from '../../../shared/channels'

/** The four messaging channels a computer can host, as the Marketplace lists them. */
export type ChannelProvider = 'whatsapp' | 'telegram' | 'slack' | 'discord'

/** One channel of a provider: a linked number, a Telegram bot, a Slack or Discord channel. */
export interface ChannelItem {
  /** `provider:channel`, stable across reloads. */
  key: string
  /** What the host calls it (`telegram`'s name, Slack/Discord's channel id). */
  channel: string
  label: string
  subtitle: string
  state: 'ok' | 'starting' | 'error' | 'off'
  stateText: string
  botId: string | null
  botName: string | null
  people: { id: string; label: string }[]
  memory: string
  icon: string
  lastReceivedAt: number | null
}

export interface ChannelProviderEntry {
  id: ChannelProvider
  name: string
  description: string
  /** Shown as the card's line under the name. */
  summary: string
  connected: boolean
  needsAttention: boolean
  items: ChannelItem[]
}

export interface ChannelSources {
  whatsapp: WhatsAppState | null
  telegram: TelegramBotStatus[]
  slack: SlackStatus | null
  discord: DiscordStatus | null
}

export const CHANNEL_PROVIDERS: { id: ChannelProvider; name: string; description: string }[] = [
  {
    id: 'whatsapp',
    name: 'WhatsApp',
    description: 'Link a WhatsApp number so your bots read and answer its chats. A separate number is safer: WhatsApp can block one it sees as automated.',
  },
  {
    id: 'telegram',
    name: 'Telegram',
    description: 'Talk to a bot from Telegram. You add the bot with the token @BotFather gave it, and only people you pair can write.',
  },
  {
    id: 'slack',
    name: 'Slack',
    description: 'Answers in the Slack channels you bind, with the people you allow. The signing secret and app token stay in this computer.',
  },
  {
    id: 'discord',
    name: 'Discord',
    description: 'Answers in your server\'s channels, and approvals work from Discord buttons. The bot token stays in this computer.',
  },
]

/** A channel's state as the shared wording says it, plus whether it wants attention. */
export function stateOf(raw: string): { state: ChannelItem['state']; text: string } {
  if (raw === 'ok' || raw === 'connected') return { state: 'ok', text: 'Connected' }
  if (raw === 'starting' || raw === 'qr') return { state: 'starting', text: raw === 'qr' ? 'Waiting for the scan' : 'Starting…' }
  if (raw === 'loggedOut') return { state: 'error', text: 'Signed out of the phone' }
  if (raw.startsWith('error')) return { state: 'error', text: raw.slice(6).trim() || 'Cannot connect' }
  return { state: 'off', text: 'Not connected' }
}

const peopleOf = (ids: (string | number)[]) => ids.map((id) => ({ id: String(id), label: `Person ${id}` }))

function bindingItems(
  provider: 'slack' | 'discord',
  bindings: { channelId: string; botId: string; allowedUsers: string[]; memory?: string; icon?: string }[] | null,
  state: string,
  botName: (id: string) => string | null,
): ChannelItem[] {
  return (bindings ?? []).map((binding) => {
    const text = stateOf(state)
    return {
      key: `${provider}:${binding.channelId}`,
      channel: binding.channelId,
      label: binding.channelId,
      subtitle: text.text,
      state: text.state,
      stateText: text.text,
      botId: binding.botId,
      botName: botName(binding.botId),
      people: peopleOf(binding.allowedUsers),
      memory: binding.memory ?? '',
      icon: binding.icon ?? '',
      lastReceivedAt: null,
    }
  })
}

/**
 * The Marketplace's channel list, from what the host already reports. Pure so the wording and the
 * attention rules can be tested without a host.
 */
export function buildChannelCatalog(sources: ChannelSources, botName: (id: string) => string | null = () => null): ChannelProviderEntry[] {
  const byId = new Map(CHANNEL_PROVIDERS.map((provider) => [provider.id, provider]))
  const entry = (id: ChannelProvider, items: ChannelItem[], connected: boolean): ChannelProviderEntry => {
    const meta = byId.get(id)!
    const needsAttention = items.some((item) => item.state === 'error')
    const summary = !connected
      ? 'Not connected'
      : items.length === 1
        ? `${items[0].label} · ${items[0].stateText}`
        : `${items.length} channels · ${needsAttention ? 'one needs attention' : 'all connected'}`
    return { id, name: meta.name, description: meta.description, summary, connected, needsAttention, items }
  }

  const whatsappState = sources.whatsapp ? stateOf(sources.whatsapp.state) : stateOf('disconnected')
  const whatsappItem: ChannelItem = {
    key: 'whatsapp:link',
    channel: 'whatsapp',
    label: sources.whatsapp?.name || sources.whatsapp?.me || 'WhatsApp number',
    subtitle: sources.whatsapp?.me ?? whatsappState.text,
    state: whatsappState.state,
    stateText: whatsappState.text,
    botId: sources.whatsapp?.bot ?? null,
    botName: sources.whatsapp?.bot ? botName(sources.whatsapp.bot) : null,
    people: peopleOf(sources.whatsapp?.allowed ?? []),
    memory: '',
    icon: '',
    lastReceivedAt: null,
  }
  const whatsappConnected = sources.whatsapp?.state === 'connected'
  const whatsapp = entry('whatsapp', whatsappConnected ? [whatsappItem] : [], whatsappConnected)

  const telegram: ChannelItem[] = sources.telegram.map((bot) => {
    const text = stateOf(bot.state)
    return {
      key: `telegram:${bot.name}`,
      channel: bot.name,
      label: bot.name,
      subtitle: [bot.username ? `@${bot.username}` : '', bot.botName].filter(Boolean).join(' · '),
      state: text.state,
      stateText: text.text,
      botId: bot.botId,
      botName: bot.botName || botName(bot.botId),
      people: peopleOf(bot.allowed),
      memory: bot.memory ?? '',
      icon: bot.icon ?? '',
      lastReceivedAt: bot.lastReceivedAt,
    }
  })

  const slackState = sources.slack?.configured ? 'ok' : 'disconnected'
  const slack = entry('slack', bindingItems('slack', sources.slack?.bindings ?? null, slackState, botName), sources.slack?.configured === true)
  const discordState = sources.discord?.state ?? 'disconnected'
  const discord = entry(
    'discord',
    bindingItems('discord', sources.discord?.bindings ?? null, sources.discord?.configured ? discordState : 'disconnected', botName),
    sources.discord?.configured === true,
  )
  return [whatsapp, entry('telegram', telegram, telegram.length > 0), slack, discord]
}

/** The channels matching a search, by name, channel or the provider's words. */
export function matchChannels(entries: ChannelProviderEntry[], query: string): ChannelProviderEntry[] {
  const q = query.trim().toLowerCase()
  if (!q) return entries
  return entries.filter((entry) =>
    `${entry.name} ${entry.description} ${entry.items.map((item) => `${item.label} ${item.subtitle} ${item.botName ?? ''}`).join(' ')}`
      .toLowerCase()
      .includes(q),
  )
}
