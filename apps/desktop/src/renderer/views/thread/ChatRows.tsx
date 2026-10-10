import { FileDownloadCard } from './FileDownloadCard'
import { memo, useEffect, useRef, useState } from 'react'
import { isGroup, isWorking, needsInput, type Bot, type Entry, type ThreadSummary } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, useContextMenu, type MenuItem, type ReactionPick } from '../../components/Overlay'
import { ThinkingOrb } from '../../components/ThinkingOrb'
import { font, px } from '../../lib/fonts'
import { avatarColor } from '../../lib/theme'
import { useStore as useLiveStore, useStoreRef as useStore } from '../../store/context'
import { QUICK_REACTIONS, type BotStore } from '../../store/bot-store'
import { AttachmentList } from './Attachments'
import { relativeTime } from './chat-items'
import { MarkdownText } from './Markdown'
import { PermissionCard } from './PermissionCard'
import { ConnectionRequestCard } from '../marketplace/ConnectionRequestCard'

export function reactionPick(store: BotStore, entry: Entry): ReactionPick {
  return { emoji: QUICK_REACTIONS, chosen: entry.data.reactions ?? [], toggle: (e) => store.react(entry, e) }
}

/** One chat-visible entry in a chat or a thread, with what can be done to it. */
export const ChatRow = memo(function ChatRow({ entry, groupStart, chat, openTrace, openThread }: {
  entry: Entry
  groupStart: boolean
  chat: Bot | null
  openTrace: () => void
  /** Main chat only: start (or open) the thread on this message. */
  openThread?: (e: Entry) => void
}) {
  const reply = openThread ? () => openThread(entry) : undefined
  const chip = entry.data.thread && entry.data.thread.count > 0 && openThread ? <ThreadChip summary={entry.data.thread} open={() => openThread(entry)} /> : null
  switch (entry.kind) {
    case 'user':
      return (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-end', gap: 4, paddingTop: groupStart ? 12 : 4 }}>
          <UserBubble entry={entry} botWorking={!!chat && isWorking(chat)} reply={reply} />
          <ReactionsRow entry={entry} />
          {chip}
        </div>
      )
    case 'agent':
      return (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'flex-start', gap: 4, paddingTop: groupStart ? 12 : 4 }}>
          {chat && isGroup(chat) && groupStart ? <AuthorLabel botId={entry.data.author} /> : null}
          <AgentBubble entry={entry} openTrace={openTrace} reply={reply} />
          <div style={{ paddingLeft: chat && isGroup(chat) ? 34 : 0, display: 'flex', flexDirection: 'column', gap: 4 }}>
            <ReactionsRow entry={entry} />
            {chip}
          </div>
        </div>
      )
    case 'permission':
      return (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 4, paddingTop: 12 }}>
          {chat && isGroup(chat) ? <AuthorLabel botId={entry.data.author} /> : null}
          <LivePermission entry={entry} />
        </div>
      )
    default:
      if (entry.data.connectionRequest) return <div style={{ paddingTop: 12 }}><ConnectionRequestCard entry={entry} /></div>
      return <div style={{ paddingTop: 10 }}><NoticeRow entry={entry} /></div>
  }
})

/** Follows the store: the card shows a spinner while its answer is on its way. */
function LivePermission({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  return <PermissionCard entry={entry} hostName={store.hostName} answering={store.answering.get(entry.id) ?? null} respond={(o) => store.respond(entry, o)} />
}

/** Who wrote a message in a group: their avatar and name in their color. */
export function AuthorLabel({ botId }: { botId?: string | null }) {
  const store = useStore()
  const bot = botId ? store.bots.get(botId) : undefined
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
      {bot ? <BotAvatar bot={bot} size={26} animated={false} /> : null}
      <span style={{ ...font('subheadline', 'medium'), color: bot ? avatarColor(bot.avatarColor) : 'var(--secondary)' }}>{store.authorName(botId)}</span>
    </div>
  )
}

/** Under a message with a thread (Slack's): who replied, how many, how recently. */
export function ThreadChip({ summary, open }: { summary: ThreadSummary; open: () => void }) {
  const store = useStore()
  const authors = summary.authors.map((id) => store.bots.get(id)).filter((b): b is Bot => !!b).slice(0, 3)
  const unread = summary.unread ?? 0
  return (
    <button
      className="press thread-chip"
      onClick={open}
      title="View thread"
      aria-label={`View thread, ${summary.count} ${summary.count === 1 ? 'reply' : 'replies'}${unread > 0 ? `, ${unread} new` : ''}`}
    >
      {authors.length ? (
        <span style={{ display: 'flex' }}>
          {authors.map((b, i) => (
            <span key={b.id} style={{ marginLeft: i ? -6 : 0 }}>
              <BotAvatar bot={b} size={18} animated={false} />
            </span>
          ))}
        </span>
      ) : null}
      <span style={{ ...font('footnote', 'semibold'), color: 'var(--accent)' }}>{summary.count === 1 ? '1 reply' : `${summary.count} replies`}</span>
      {unread > 0 ? (
        <span style={{ ...font('footnote', 'semibold'), color: 'var(--text)' }}>{unread} new</span>
      ) : (
        <span style={{ ...font('footnote'), color: 'var(--tertiary)' }}>{relativeTime.day(summary.lastAt)}</span>
      )}
      <Icon name="chevron.right" size={10} color="var(--tertiary)" />
    </button>
  )
}

/** A message's text with a right-click menu (with quick reactions on top). */
export function Bubble({ className, items, reactions, children }: { className: string; items: () => MenuItem[]; reactions?: ReactionPick; children: React.ReactNode }) {
  const menu = useContextMenu()
  return (
    <>
      <div className={className} onContextMenu={menu.onContextMenu}>
        {children}
      </div>
      <AnchoredMenu open={menu.open} onClose={menu.close} point={menu.point} items={items} reactions={reactions} />
    </>
  )
}

/** The centered time label between runs of messages. */
export function TimeSeparator({ date }: { date: number }) {
  return <div style={{ ...font('footnote'), color: 'var(--tertiary)', textAlign: 'center', padding: '18px 0 6px' }}>{relativeTime.separator(date)}</div>
}

const fullDate = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })

export function UserBubble({ entry, botWorking, reply }: { entry: Entry; botWorking: boolean; reply?: () => void }) {
  const store = useStore()
  const [hovering, setHovering] = useState(false)
  const status = entry.data.status
  const copy = () => entry.data.text && window.codync.app.copy(entry.data.text)
  return (
    <div className="message user" title={fullDate.format(entry.createdAt)} onMouseEnter={() => setHovering(true)} onMouseLeave={() => setHovering(false)}>
      <div className="message-body">
        {entry.data.attachments?.length ? <AttachmentList attachments={entry.data.attachments} botId={entry.botId} /> : null}
        {entry.data.text ? (
          <Bubble
            className="bubble user selectable"
            reactions={reactionPick(store, entry)}
            items={() => [
              { title: 'Copy', icon: 'square.on.square', action: copy },
              ...(reply ? [{ title: 'Reply in thread', icon: 'arrowshape.turn.up.left', action: reply }] : []),
            ]}
          >
            {entry.data.text}
          </Bubble>
        ) : null}
        {status && status !== 'sent' ? <UserStatus entry={entry} botWorking={botWorking} hovering={hovering} /> : null}
        {/* Beside the bubble, inside its hover region: showing them never changes the height. */}
        <MessageActions className="leading" visible={hovering} reactions={reactionPick(store, entry)} reply={reply} copy={copy} />
      </div>
    </div>
  )
}

function UserStatus({ entry, botWorking, hovering }: { entry: Entry; botWorking: boolean; hovering: boolean }) {
  const store = useStore()
  const caption = { ...font('caption2'), color: 'var(--tertiary)' }
  switch (entry.data.status) {
    case 'sending':
      return <span style={caption}>Sending…</span>
    case 'queued':
      return <span style={caption}>{botWorking ? 'Queued until this response finishes' : 'Queued'}</span>
    case 'failed':
      return (
        <span style={{ display: 'flex', alignItems: 'center', gap: 10, ...font('caption2', 'semibold') }}>
          <span style={{ color: 'var(--danger)' }}>Failed to send</span>
          <button title="Resend" aria-label="Resend" style={{ display: 'flex' }} onClick={() => store.retry(entry)}>
            <Icon name="arrow.clockwise" size={10} weight="semibold" />
          </button>
          <button title="Delete" aria-label="Delete" style={{ display: 'flex' }} onClick={() => store.discard(entry)}>
            <Icon name="trash" size={10} weight="semibold" />
          </button>
        </span>
      )
    case 'cancelled':
      return <span style={caption}>Not sent — stopped</span>
    case 'waiting':
      return (
        <span style={{ display: 'flex', alignItems: 'center', gap: 10, ...font('caption2') }}>
          <span style={{ color: 'var(--tertiary)' }}>Waiting for the computer to come online</span>
          <button title="Don't send" aria-label="Don't send" style={{ display: 'flex' }} onClick={() => store.cancelQueued(entry)}>
            <Icon name="xmark.circle" size={10} />
          </button>
        </span>
      )
    case 'delivering':
      return <span style={caption}>Delivered to the computer</span>
    default:
      return <span style={{ ...font(10), color: 'var(--tertiary)', opacity: hovering ? 1 : 0, transition: 'opacity var(--hover)' }}>{relativeTime.time(entry.createdAt)}</span>
  }
}

export function AgentBubble({ entry, openTrace, reply }: { entry: Entry; openTrace: () => void; reply?: () => void }) {
  const store = useStore()
  const [hovering, setHovering] = useState(false)
  const copy = () => entry.data.text && window.codync.app.copy(entry.data.text)
  return (
    <div className="message agent" title={fullDate.format(entry.createdAt)} onMouseEnter={() => setHovering(true)} onMouseLeave={() => setHovering(false)}>
      <div className="message-body">
        <Bubble
          className={entry.data.files?.length ? "file-message" : "bubble agent"}
          reactions={reactionPick(store, entry)}
          items={() => [
            { title: 'Copy', icon: 'square.on.square', action: copy },
            ...(reply ? [{ title: 'Reply in thread', icon: 'arrowshape.turn.up.left', action: reply }] : []),
            { title: 'Show what it did', icon: 'list.bullet', action: openTrace },
          ]}
        >
          {entry.data.files?.length ? <div style={{ display: 'grid', gap: 6 }}>{entry.data.files.map((file) => <FileDownloadCard key={file.id} entry={entry} file={file} />)}</div> : <MarkdownText text={entry.data.text ?? ''} />}
        </Bubble>
        <MessageActions className="trailing" visible={hovering} reactions={reactionPick(store, entry)} reply={reply} trace={openTrace} copy={copy} />
      </div>
    </div>
  )
}

export function NoticeRow({ entry }: { entry: Entry }) {
  const text = entry.data.text ?? ''
  if (entry.data.callSeconds !== undefined) {
    const s = entry.data.callSeconds
    return (
      <div style={{ display: 'flex', justifyContent: 'center', alignItems: 'center', gap: 6, ...font('footnote'), fontVariantNumeric: 'tabular-nums', color: 'var(--secondary)' }}>
        <Icon name="waveform" size={10} />
        {`${text} · ${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`}
      </div>
    )
  }
  switch (entry.data.style) {
    case 'divider':
      return (
        <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '6px 0' }}>
          <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
          <span style={{ ...font('caption2'), color: 'var(--tertiary)', textAlign: 'center', maxWidth: 260 }}>{text}</span>
          <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
        </div>
      )
    case 'error':
      return (
        <div style={{ display: 'flex', alignItems: 'flex-start', gap: 8, padding: 12, borderRadius: 12, background: 'color-mix(in srgb, var(--danger) 12%, transparent)' }}>
          <Icon name="exclamationmark.triangle.fill" size={10} color="var(--danger)" />
          <span className="selectable" style={{ ...font('footnote'), color: 'var(--text)' }}>{text}</span>
        </div>
      )
    default:
      return <div style={{ ...font('footnote'), color: 'var(--secondary)', textAlign: 'center' }}>{text}</div>
  }
}

/**
 * The bot's live activity line: a turning orb, what it's doing and for how long. Clicking it
 * unfolds what the bot is thinking right now.
 */
export function WorkingIndicator({ bot, thinking }: { bot: Bot; thinking: string | null }) {
  const [expanded, setExpanded] = useState(false)
  const waiting = needsInput(bot)
  return (
    <div style={{ display: 'flex', alignItems: 'center', gap: 10, paddingRight: 40 }}>
      <button className="working" disabled={!thinking} onClick={() => setExpanded((e) => !e)} aria-expanded={thinking ? expanded : undefined}>
        <span style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
          <ThinkingOrb state={waiting ? 'listening' : 'working'} size={16} color={waiting ? 'var(--warning)' : 'var(--secondary)'} />
          <span style={{ ...font('subheadline'), color: waiting ? 'var(--warning)' : 'var(--secondary)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{bot.activity || 'Working…'}</span>
          {bot.startedAt ? <Timer since={bot.startedAt} /> : null}
          {thinking ? (
            <span style={{ display: 'flex', transform: `rotate(${expanded ? 180 : 0}deg)`, transition: 'transform var(--layout)' }}>
              <Icon name="chevron.down" size={10} weight="semibold" color="var(--tertiary)" />
            </span>
          ) : null}
        </span>
        {expanded && thinking ? (
          <div className="selectable thinking" style={{ ...font('footnote'), color: 'var(--secondary)' }} ref={(el) => {
            if (el) el.scrollTop = el.scrollHeight
          }}>
            {thinking}
          </div>
        ) : null}
      </button>
    </div>
  )
}

/** `Text(date, style: .timer)`: 0:42, 12:05, 1:02:33. */
function Timer({ since }: { since: number }) {
  const [now, setNow] = useState(Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [])
  const s = Math.max(0, Math.floor((now - since) / 1000))
  const h = Math.floor(s / 3600)
  const m = Math.floor((s % 3600) / 60)
  const sec = String(s % 60).padStart(2, '0')
  return <span style={{ ...font('footnote'), fontVariantNumeric: 'tabular-nums', color: 'var(--tertiary)' }}>{h ? `${h}:${String(m).padStart(2, '0')}:${sec}` : `${m}:${sec}`}</span>
}

/** The user's reactions under a message; clicking one takes it back. */
export function ReactionsRow({ entry }: { entry: Entry }) {
  const store = useStore()
  const reactions = entry.data.reactions
  if (!reactions?.length) return null
  return (
    <div style={{ display: 'flex', gap: 4 }}>
      {reactions.map((emoji) => (
        <button key={emoji} className="press reaction-chip" title={`Remove ${emoji}`} aria-label={`Remove reaction ${emoji}`} onClick={() => store.react(entry, emoji)}>
          <span style={{ fontSize: px(13) }}>{emoji}</span>
        </button>
      ))}
    </div>
  )
}

/** Compact actions beside the bubble: a reaction strip, reply in thread, and more (copy, what it did). */
function MessageActions({ visible, className, reactions, reply, trace, copy }: {
  visible: boolean
  className: string
  reactions: ReactionPick
  reply?: () => void
  trace?: () => void
  copy: () => void
}) {
  const [menu, setMenu] = useState<'reactions' | 'more' | null>(null)
  const smile = useRef<HTMLButtonElement>(null)
  const more = useRef<HTMLButtonElement>(null)
  const shown = visible || menu !== null
  return (
    <div className={`message-actions ${className}`} style={{ opacity: shown ? 1 : 0, pointerEvents: shown ? 'auto' : 'none' }} aria-hidden={!shown}>
      <button ref={smile} className="message-action" title="Add reaction" aria-label="Add reaction" onClick={() => setMenu('reactions')}>
        <Icon name="face.smiling" size={14} />
      </button>
      {reply ? <IconButton title="Reply in thread" icon="arrowshape.turn.up.left" size={26} onClick={reply} /> : null}
      <button ref={more} className="message-action" title="More message actions" aria-label="More message actions" onClick={() => setMenu('more')}>
        <Icon name="ellipsis" size={14} />
      </button>
      <AnchoredMenu open={menu === 'reactions'} onClose={() => setMenu(null)} anchor={smile} items={() => []} reactions={reactions} />
      <AnchoredMenu
        open={menu === 'more'}
        onClose={() => setMenu(null)}
        anchor={more}
        items={() => [{ title: 'Copy', icon: 'square.on.square', action: copy }, ...(trace ? [{ title: 'Show what it did', icon: 'list.bullet', action: trace }] : [])]}
      />
    </div>
  )
}
