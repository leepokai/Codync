import { useEffect, useRef } from 'react'
import { BotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { botConversation, buildBotConversation, groupSummary, verbOf, type BotConversationRow, type BotExchange } from './bot-exchange'
import { AuthorLabel, Bubble, TimeSeparator } from './ChatRows'
import { MarkdownText } from './Markdown'
import './bot-conversation.css'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'


/** The compact chat row for bot-to-bot messages with one peer: who, never what. Opens the conversation. */
export function BotMessageRow({ exchanges, open }: { exchanges: BotExchange[]; open: (peerId: string) => void }) {
  const store = useStore()
  const { peerId, count, failed } = groupSummary(exchanges)
  const peer = store.bots.get(peerId)
  const name = store.authorName(peerId)
  const lead = count > 1 ? `${count} messages with` : verbOf(exchanges[0]!)
  return (
    <button
      className={failed ? 'press bot-message-row failed' : 'press bot-message-row'}
      onClick={() => open(peerId)}
      title="Open conversation"
      aria-label={`${lead} ${name}${failed ? ', failed' : ''}`}
    >
      {failed ? <Icon name="exclamationmark.triangle.fill" size={10} color="var(--danger)" /> : null}
      <Text size="footnote">{lead}</Text>
      {peer ? <BotAvatar bot={peer} size={18} animated={false} /> : null}
      <Text size="footnote" weight="semibold" tone={failed ? 'danger' : 'text'}>{name}</Text>
      <Icon name="chevron.right" size={10} color="var(--tertiary)" />
    </button>
  )
}

/** The pair a conversation is between, for its header: the chat's own bot, then its peer. */
function BotPair({ botId, peerId }: { botId: string; peerId: string }) {
  const store = useStore()
  const label = (id: string) => (
    <HStack as="span" gap={6} shrink>
      {store.bots.get(id) ? <BotAvatar bot={store.bots.get(id)!} size={20} animated={false} /> : null}
      <Text truncate>{store.authorName(id)}</Text>
    </HStack>
  )
  return (
    <HStack as="span" gap={8} aria-label={`${store.authorName(botId)} and ${store.authorName(peerId)}`}>
      {label(botId)}
      <Icon name="arrow.left.arrow.right" size={11} color="var(--tertiary)" />
      {label(peerId)}
    </HStack>
  )
}

/** Read-only history between a bot and one peer: the live chat merged with what the host returns. */
export function BotConversationView({ botId, peerId }: { botId: string; peerId: string }) {
  const tr = useTr()
  const store = useStore()
  // null while the host's history loads.
  const { fetched, error } = useFetched(() => store.botConversation(botId, peerId), [store, botId, peerId])
  const scroller = useRef<HTMLDivElement>(null)
  const rows = buildBotConversation(botConversation(fetched ?? [], store.allEntries(botId), peerId))

  useEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [rows.length, rows[rows.length - 1]?.id])

  return (
    <div className="bot-conversation">
      <ModalHeader title={<BotPair botId={botId} peerId={peerId} />} />
      <div className="bot-conversation-scroll" ref={scroller}>
        <div className="bot-conversation-rows">
          {error ? <Text as="div" size="footnote" tone="danger" className="bot-conversation-note">{error}</Text> : null}
          {!error && fetched && rows.length === 0 ? <Text as="div" size="footnote" tone="tertiary" className="bot-conversation-note">{tr(`No messages yet.`)}</Text> : null}
          {rows.map((row) => <ConversationRow key={row.id} row={row} />)}
        </div>
      </div>
    </div>
  )
}

function ConversationRow({ row }: { row: BotConversationRow }) {
  const store = useStore()
  switch (row.kind) {
    case 'separator':
      return <TimeSeparator date={row.at} />
    case 'message':
      return (
        <VStack align="flex-start" gap={4} className={row.showsAuthor ? 'chat-row group-start' : 'chat-row'}>
          {row.showsAuthor ? <AuthorLabel botId={row.author} /> : null}
          <div className="message agent">
            <div className="message-body">
              <Bubble className="bubble agent selectable" items={() => [{ title: 'Copy', icon: 'square.on.square', action: () => window.codync.app.copy(row.text) }]}>
                <MarkdownText text={row.text} />
              </Bubble>
            </div>
          </div>
        </VStack>
      )
    case 'status': {
      const failed = row.outcome.kind === 'failed'
      const text = row.outcome.kind === 'failed' ? row.outcome.detail || "Didn't complete." : `Waiting for ${store.authorName(row.awaiting)}…`
      return <Text as="div" size="footnote" tone={failed ? 'danger' : 'secondary'} className="selectable bot-conversation-status">{text}</Text>
    }
  }
}
