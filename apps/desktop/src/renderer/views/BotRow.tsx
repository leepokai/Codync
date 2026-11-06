import { memo } from 'react'
import { folderName, isGroup, isWorking, needsInput, type Bot } from '@shared/models'
import { AvatarWithStatus } from '../components/Avatar'
import { Icon } from '../components/Icon'
import { HStack, VStack } from '../components/Stack'
import { Text } from '../components/Text'
import { ThinkingOrb } from '../components/ThinkingOrb'
import { messagePreview } from '../lib/message-preview'
import { useModel } from '../lib/observable'
import { useTr } from '../lib/use-language'
import type { BotStore } from '../store/bot-store'

/** A roster row: avatar with status, name, and the live activity or last message (Grok Bot's desktop sidebar). */
export const BotRow = memo(function BotRow({ bot, store, compact, usingComputer, members }: { bot: Bot; store: BotStore; compact: boolean; usingComputer: boolean; members: Bot[] }) {
  // The cached roster paints before hello names the agents. Follow the store so that arrival replaces "Custom agent".
  useModel(store)
  return (
    <HStack gap={compact ? 0 : 10} justify={compact ? 'center' : 'flex-start'} className="bot-row">
      <AvatarWithStatus bot={bot} members={members} size={30} />
      <VStack gap={2} className={`bot-row-text ${compact ? 'collapsed' : ''}`} aria-hidden={compact}>
        <HStack align="baseline" gap={4} shrink>
          {bot.pinned ? <Icon name="pin.fill" size={10} color="var(--tertiary)" /> : null}
          <Text size="body" weight={bot.unread > 0 ? 'semibold' : 'medium'} tone="text" truncate>{bot.name}</Text>
          {usingComputer ? <Icon name="cursorarrow.motionlines" size={10} color="var(--accent)" title="Using the computer" /> : null}
        </HStack>
        <HStack gap={6} shrink>
          <HStack align="stretch" grow>
            <Preview bot={bot} store={store} />
          </HStack>
          {bot.unread > 0 ? <span className="unread-badge">{bot.unread}</span> : null}
        </HStack>
      </VStack>
    </HStack>
  )
})

/** Live activity while working, otherwise the last message (Grok Bot row behavior). */
function Preview({ bot, store }: { bot: Bot; store: BotStore }) {
  const tr = useTr()
  const place = bot.managedWorkspace ? tr('Personal workspace') : folderName(bot)
  if (needsInput(bot)) {
    return (
      <HStack as="span" gap={6} shrink>
        <ThinkingOrb state="listening" size={16} color="var(--warning)" />
        <Text size="callout" tone="attention" truncate>{tr(bot.activity || 'Needs your approval')}</Text>
      </HStack>
    )
  }
  if (isWorking(bot)) {
    return (
      <HStack as="span" gap={6} shrink>
        <ThinkingOrb size={13} color="var(--secondary)" />
        <Text size="callout" tone="secondary" truncate>{tr(bot.activity || 'Working…')}</Text>
      </HStack>
    )
  }
  if (bot.status === 'error') return <Text size="callout" tone="danger" truncate>{messagePreview(bot.lastMessage ?? tr(`Something went wrong`))}</Text>
  return <Text size="callout" tone="secondary" truncate>{messagePreview(bot.lastMessage ?? (isGroup(bot) ? `${bot.members.length} bots` : `${store.backendName(bot.backend)} · ${place}`))}</Text>
}
