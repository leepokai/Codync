import { memo } from 'react'
import { folderName, isGroup, type Bot } from '@shared/models'
import { rosterLine } from '@shared/work'
import { AvatarWithStatus } from '../components/Avatar'
import { Icon } from '../components/Icon'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { ThinkingOrb } from '../components/ThinkingOrb'
import { UnreadBadge } from '../components/UnreadBadge'
import { messagePreview } from '../lib/message-preview'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import type { BotStore } from '../store/bot-store'

/** A roster row: avatar with status, name, and the live activity or last message (Grok Bot's desktop sidebar).
 * `agent` is the one whose logo marks the avatar, if any. */
export const BotRow = memo(function BotRow({ bot, store, usingComputer, members, agent }: { bot: Bot; store: BotStore; usingComputer: boolean; members: Bot[]; agent: string | null }) {
  // The cached roster paints before hello names the agents. Follow the store so that arrival replaces "Custom agent".
  useModel(store)
  const paused = bot.paused && !isGroup(bot)
  return (
    <HStack gap={10} className={`bot-row${paused ? ' paused' : ''}`}>
      {/* The roster row's own label already names the pause (BotRoster), so its mark stays decorative. */}
      <AvatarWithStatus bot={bot} members={members} size={30} agent={agent ? store.agentRegistry(agent) : null} announcePaused={false} />
      <VStack gap={2} className="bot-row-text">
        <HStack align="baseline" gap={4} shrink>
          {bot.pinned ? <Icon name="pin.fill" size={10} color="var(--tertiary)" /> : null}
          <Text size="body" weight={bot.lead ? 'bold' : bot.unread > 0 ? 'semibold' : 'medium'} tone="text" truncate>{bot.name}</Text>
          {usingComputer ? <Icon name="cursorarrow.motionlines" size={10} color="var(--accent)" title="Using the computer" /> : null}
        </HStack>
        <HStack gap={6} shrink>
          <HStack align="stretch" grow>
            <Preview bot={bot} store={store} />
          </HStack>
          {bot.unread > 0 ? <UnreadBadge count={bot.unread} waiting={bot.waiting > 0} /> : null}
        </HStack>
      </VStack>
    </HStack>
  )
})

/** The most pressing thing first: it needs you, what its chat does, how many tasks work and wait;
 * otherwise the last message (Grok Bot row behavior). */
function Preview({ bot, store }: { bot: Bot; store: BotStore }) {
  const tr = useTr()
  const place = folderName(bot, tr)
  const line = rosterLine(bot, tr)
  if (bot.paused && !isGroup(bot)) {
    return <Text size="callout" tone="secondary" truncate>{tr(`Paused`)}</Text>
  }
  if (line?.kind === 'needsYou') {
    return (
      <HStack as="span" gap={6} shrink>
        <ThinkingOrb state="listening" size={16} color="var(--warning)" />
        <Text size="callout" tone="attention" truncate>{line.text}</Text>
      </HStack>
    )
  }
  if (line?.kind === 'working') {
    return (
      <HStack as="span" gap={6} shrink>
        <ThinkingOrb size={13} color="var(--secondary)" />
        <Text size="callout" tone="secondary" truncate>{line.text}</Text>
      </HStack>
    )
  }
  if (line) return <Text size="callout" tone="secondary" truncate>{line.text}</Text>
  if (bot.status === 'error') return <Text size="callout" tone="danger" truncate>{messagePreview(bot.lastMessage ?? tr(`Something went wrong`))}</Text>
  return <Text size="callout" tone="secondary" truncate>{messagePreview(bot.lastMessage ?? (isGroup(bot) ? `${bot.members.length} bots` : `${store.backendName(bot.backend)} · ${place}`))}</Text>
}
