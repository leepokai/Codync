import { useEffect, useRef, useState } from 'react'
import { BotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import { ModalHeader } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import type { Entry } from '@shared/models'
import { botConversation, buildBotConversation, groupSummary, verbOf, type BotConversationRow, type BotExchange } from './bot-exchange'
import { AuthorLabel, Bubble, TimeSeparator } from './ChatRows'
import { MarkdownText } from './Markdown'
import './bot-conversation.css'

/** The compact chat row for bot-to-bot messages with one peer: who, never what. Opens the conversation. */
export function BotMessageRow({ exchanges, open }: { exchanges: BotExchange[]; open: (peerId: string) => void }) {
  const store = useStore()
  const { peerId, count, failed } = groupSummary(exchanges)
  const peer = store.bots.get(peerId)
  const name = store.authorName(peerId)
  const lead = count > 1 ? `${count} messages with` : verbOf(exchanges[0]!)
  return (
    <button
      className="press bot-message-row"
      onClick={() => open(peerId)}
      title="Open conversation"
      aria-label={`${lead} ${name}${failed ? ', failed' : ''}`}
      style={{ ...font('footnote'), color: failed ? 'var(--danger)' : 'var(--secondary)' }}
    >
      {failed ? <Icon name="exclamationmark.triangle.fill" size={10} color="var(--danger)" /> : null}
      <span>{lead}</span>
      {peer ? <BotAvatar bot={peer} size={18} animated={false} /> : null}
      <span style={{ ...font('footnote', 'semibold'), color: failed ? 'var(--danger)' : 'var(--text)' }}>{name}</span>
      <Icon name="chevron.right" size={10} color="var(--tertiary)" />
    </button>
  )
}

/** The pair a conversation is between, for its header: the chat's own bot, then its peer. */
function BotPair({ botId, peerId }: { botId: string; peerId: string }) {
  const store = useStore()
  const label = (id: string) => (
    <span style={{ display: 'flex', alignItems: 'center', gap: 6, minWidth: 0 }}>
      {store.bots.get(id) ? <BotAvatar bot={store.bots.get(id)!} size={20} animated={false} /> : null}
      <span style={{ overflow: 'hidden', textOverflow: 'ellipsis' }}>{store.authorName(id)}</span>
    </span>
  )
  return (
    <span style={{ display: 'flex', alignItems: 'center', gap: 8 }} aria-label={`${store.authorName(botId)} and ${store.authorName(peerId)}`}>
      {label(botId)}
      <Icon name="arrow.left.arrow.right" size={11} color="var(--tertiary)" />
      {label(peerId)}
    </span>
  )
}

/** Read-only history between a bot and one peer: the live chat merged with what the host returns. */
export function BotConversationView({ botId, peerId }: { botId: string; peerId: string }) {
  const store = useStore()
  // null while the host's history loads.
  const [fetched, setFetched] = useState<Entry[] | null>(null)
  const [error, setError] = useState<string | null>(null)
  const scroller = useRef<HTMLDivElement>(null)
  const rows = buildBotConversation(botConversation(fetched ?? [], store.chat(botId), peerId))

  useEffect(() => {
    let current = true
    store.botConversation(botId, peerId).then(
      (entries) => {
        if (current) setFetched(entries)
      },
      (reason: unknown) => {
        console.warn('botConversation failed', reason)
        if (!current) return
        setFetched([])
        setError(reason instanceof Error ? reason.message : String(reason))
      },
    )
    return () => {
      current = false
    }
  }, [store, botId, peerId])

  useEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [rows.length, rows[rows.length - 1]?.id])

  return (
    <div className="bot-conversation">
      <ModalHeader title={<BotPair botId={botId} peerId={peerId} />} />
      <div className="bot-conversation-scroll" ref={scroller}>
        <div className="bot-conversation-rows">
          {error ? <div style={{ ...font('footnote'), color: 'var(--danger)', textAlign: 'center', paddingTop: 24 }}>{error}</div> : null}
          {!error && fetched && rows.length === 0 ? <div style={{ ...font('footnote'), color: 'var(--tertiary)', textAlign: 'center', paddingTop: 24 }}>No messages yet.</div> : null}
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
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 4, paddingTop: row.showsAuthor ? 12 : 4 }}>
          {row.showsAuthor ? <AuthorLabel botId={row.author} /> : null}
          <div className="message agent">
            <div className="message-body">
              <Bubble className="bubble agent selectable" items={() => [{ title: 'Copy', icon: 'square.on.square', action: () => window.codync.app.copy(row.text) }]}>
                <MarkdownText text={row.text} />
              </Bubble>
            </div>
          </div>
        </div>
      )
    case 'status': {
      const failed = row.outcome.kind === 'failed'
      const text = row.outcome.kind === 'failed' ? row.outcome.detail || "Didn't complete." : `Waiting for ${store.authorName(row.awaiting)}…`
      return <div className="selectable" style={{ ...font('footnote'), color: failed ? 'var(--danger)' : 'var(--secondary)', padding: '6px 12px' }}>{text}</div>
    }
  }
}
