import { FileDownloadCard, VoiceNotePlayer } from './FileDownloadCard'
import { ArtifactCard } from '../library/ArtifactCard'
import { memo, useRef, useState } from 'react'
import { groupBotReactions, isGroup, isReplyable, voiceNoteOf, type Bot, type Entry, type ReplyQuote, type SharedFile } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, useContextMenu, type MenuItem, type ReactionPick } from '../../components/Menu'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { messagePreview } from '../../lib/message-preview'
import { useTr } from '../../lib/use-language'
import { avatarColor } from '../../lib/theme'
import { useStore as useLiveStore, useStoreRef as useStore } from '../../store/context'
import { QUICK_REACTIONS, type BotStore } from '../../store/bot-store'
import { AttachmentList } from './Attachments'
import { copyMessageText, messageCopyItems, type MessageMenu } from './use-image-actions'
import { callLength, relativeTime } from './chat-items'
import { downloadItem } from './file-card'
import { MarkdownText } from './Markdown'
import { DecisionCard } from './DecisionCard'
import { PermissionCard } from './PermissionCard'
import { ConnectionRequestCard } from '../marketplace/ConnectionRequestCard'
import { AgentSwitchNotice, HandoffNotice } from './HandoffNotice'

export function reactionPick(store: BotStore, entry: Entry): ReactionPick {
  return { emoji: QUICK_REACTIONS, chosen: entry.data.reactions ?? [], toggle: (e) => store.react(entry, e) }
}

/** One chat-visible entry, with what can be done to it. */
export const ChatRow = memo(function ChatRow({ entry, groupStart, chat, openTrace, onReply, openAgent, openArtifact }: {
  entry: Entry
  groupStart: boolean
  chat: Bot | null
  openTrace: () => void
  /** Quotes this message in the composer; absent where the computer can't take replies. */
  onReply?: (e: Entry) => void
  /** Opens a bot's Agent settings (a failure notice's "Add a fallback"). */
  openAgent?: (botId: string) => void
  /** Opens an artifact card's artifact over the chat. */
  openArtifact?: (artifactId: string) => void
}) {
  const reply = onReply && isReplyable(entry) ? () => onReply(entry) : undefined
  switch (entry.kind) {
    case 'user':
      return (
        <VStack align="flex-end" gap={4} className={groupStart ? 'chat-row group-start' : 'chat-row'}>
          <UserBubble entry={entry} reply={reply} />
          <ReactionsRow entry={entry} />
        </VStack>
      )
    case 'agent':
      return (
        <VStack align="flex-start" gap={4} className={groupStart ? 'chat-row group-start' : 'chat-row'}>
          {chat && isGroup(chat) && groupStart ? <AuthorLabel botId={entry.data.author} /> : null}
          <AgentBubble entry={entry} openTrace={openTrace} reply={reply} openArtifact={openArtifact} />
          <VStack gap={4} className={chat && isGroup(chat) ? 'chat-row-indent' : undefined}>
            <ReactionsRow entry={entry} />
          </VStack>
        </VStack>
      )
    case 'permission':
      return (
        <VStack gap={4} className="chat-row group-start">
          {chat && isGroup(chat) ? <AuthorLabel botId={entry.data.author} /> : null}
          <LivePermission entry={entry} />
        </VStack>
      )
    default:
      if (entry.kind === 'notice' && entry.data.decision) return <div className="chat-row group-start"><LiveDecision entry={entry} /></div>
      if (entry.data.connectionRequest) return <div className="chat-row group-start"><ConnectionRequestCard entry={entry} /></div>
      if (entry.data.handoff) return <div className="chat-row notice"><HandoffNotice entry={entry} card={entry.data.handoff} /></div>
      if (entry.data.agentSwitch) return <div className="chat-row notice"><AgentSwitchNotice entry={entry} carried={entry.data.agentSwitch.carried} /></div>
      return <div className="chat-row notice"><NoticeRow entry={entry} addFallback={entry.data.addFallback && openAgent ? () => openAgent(entry.data.author ?? entry.botId) : undefined} /></div>
  }
})

/** Follows the store: the card shows a spinner while its answer is on its way. */
function LivePermission({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  return <PermissionCard entry={entry} hostName={store.hostName} answering={store.answering.get(entry.id) ?? null} respond={(o) => store.respond(entry, o)} />
}

/** The user's press on a choice card is on its way to the computer. */
function LiveDecision({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  return <DecisionCard entry={entry} answering={store.answering.get(entry.id) ?? null} respond={(option) => store.decide(entry, option)} />
}

/** Who wrote a message: their avatar and name in their color, or `name` for a bot on another computer. */
export function AuthorLabel({ botId, name }: { botId?: string | null; name?: string }) {
  const store = useStore()
  const bot = botId ? store.bots.get(botId) : undefined
  return (
    <HStack gap={8}>
      {bot ? <BotAvatar bot={bot} size={26} animated={false} /> : null}
      <Text size="subheadline" weight="medium" tone="secondary" style={bot ? { color: avatarColor(bot.avatarColor) } : undefined}>{store.authorName(botId, name)}</Text>
    </HStack>
  )
}

/** The message a reply quotes, as its first line without Markdown markers: at the top of its bubble, and in the composer while replying. */
export function QuoteRow({ quote, className }: { quote: ReplyQuote; className?: string }) {
  return (
    <HStack gap={6} className={className ?? 'bubble-quote'}>
      <Icon name="text.bubble" size={12} color="var(--secondary)" />
      <Text size="footnote" tone="secondary" truncate grow>{`“${messagePreview(quote.text)}”`}</Text>
    </HStack>
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
  const tr = useTr()
  return <Text as="div" size="footnote" tone="tertiary" className="time-separator">{relativeTime.separator(date, tr)}</Text>
}

const fullDate = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' })

export function UserBubble({ entry, reply }: { entry: Entry; reply?: () => void }) {
  const store = useStore()
  const [hovering, setHovering] = useState(false)
  const status = entry.data.status
  // A picture here has the same menu as the bubble: reactions and Reply, its own copy items in place of Copy.
  const menu: MessageMenu = { items: () => (reply ? [{ title: 'Reply', icon: 'arrowshape.turn.up.left', action: reply }] : []), reactions: reactionPick(store, entry) }
  return (
    <div className="message user" title={fullDate.format(entry.createdAt)} onMouseEnter={() => setHovering(true)} onMouseLeave={() => setHovering(false)}>
      <div className="message-body">
        {entry.data.attachments?.length ? <AttachmentList attachments={entry.data.attachments} botId={entry.botId} menu={menu} /> : null}
        {entry.data.text || entry.data.replyTo ? (
          <Bubble
            className="bubble user selectable"
            reactions={reactionPick(store, entry)}
            items={() => [
              { title: 'Copy', icon: 'square.on.square', action: () => copyMessageText(entry) },
              ...(reply ? [{ title: 'Reply', icon: 'arrowshape.turn.up.left', action: reply }] : []),
            ]}
          >
            {entry.data.replyTo ? <QuoteRow quote={entry.data.replyTo} /> : null}
            <span data-find-text>{entry.data.text}</span>
          </Bubble>
        ) : null}
        {status && status !== 'sent' ? <UserStatus entry={entry} hovering={hovering} /> : null}
        {/* Beside the bubble, inside its hover region: showing them never changes the height. */}
        <MessageActions className="leading" visible={hovering} reactions={reactionPick(store, entry)} reply={reply} copy={messageCopyItems(store, entry)} />
      </div>
    </div>
  )
}

function UserStatus({ entry, hovering }: { entry: Entry; hovering: boolean }) {
  const tr = useTr()
  const store = useStore()
  switch (entry.data.status) {
    case 'sending':
      return <Text size="caption2" tone="tertiary">{tr(`Sending…`)}</Text>
    case 'failed':
      return (
        <HStack as="span" gap={2}>
          <Text size="caption2" weight="semibold" tone="danger" className="user-status-error">{tr(`Failed to send`)}</Text>
          <IconButton title="Resend" icon="arrow.clockwise" size={24} onClick={() => store.retry(entry)} />
          <IconButton title="Delete" icon="trash" size={24} onClick={() => store.discard(entry)} />
        </HStack>
      )
    case 'cancelled':
      return <Text size="caption2" tone="tertiary">{tr(`Not sent — stopped`)}</Text>
    default:
      return <Text size="caption" tone="tertiary" className={hovering ? 'user-status-time shown' : 'user-status-time'}>{relativeTime.time(entry.createdAt)}</Text>
  }
}

export function AgentBubble({ entry, openTrace, reply, openArtifact }: { entry: Entry; openTrace: () => void; reply?: () => void; openArtifact?: (artifactId: string) => void }) {
  const store = useStore()
  const [hovering, setHovering] = useState(false)
  const voiceNote = voiceNoteOf(entry.data)
  const reactions = reactionPick(store, entry)
  const rest = (): MenuItem[] => [
    ...(reply ? [{ title: 'Reply', icon: 'arrowshape.turn.up.left', action: reply }] : []),
    { title: 'Show what it did', icon: 'list.bullet', action: openTrace },
  ]
  // A shared picture or voice note has the same menu as the bubble: reactions, Reply and what it did, its own items in place of Copy.
  const menu: MessageMenu = { items: rest, reactions }
  const download = (file: SharedFile) => downloadItem(store.fileDownloads.states.get(file.id), () => store.fileDownloads.toggle(store.client, entry.id, file))
  return (
    <div className="message agent" title={fullDate.format(entry.createdAt)} onMouseEnter={() => setHovering(true)} onMouseLeave={() => setHovering(false)}>
      <div className="message-body">
        <Bubble
          className={!voiceNote && (entry.data.files?.length || entry.data.artifact) ? 'file-message' : 'bubble agent'}
          reactions={reactions}
          items={() => [...messageCopyItems(store, entry), ...(voiceNote ? [download(voiceNote)] : []), ...rest()]}
        >
          {entry.data.artifact ? (
            <ArtifactCard botId={entry.botId} card={entry.data.artifact} open={() => openArtifact?.(entry.data.artifact!.id)} />
          ) : voiceNote ? (
            <>
              <VoiceNotePlayer entry={entry} file={voiceNote} menu={menu} />
              <MarkdownText text={entry.data.text ?? ''} findable />
            </>
          ) : entry.data.files?.length ? (
            <div className="file-message-cards">{entry.data.files.map((file) => <FileDownloadCard key={file.id} entry={entry} file={file} menu={menu} />)}</div>
          ) : (
            <MarkdownText text={entry.data.text ?? ''} findable />
          )}
        </Bubble>
        <MessageActions className="trailing" visible={hovering} reactions={reactions} reply={reply} trace={openTrace} copy={messageCopyItems(store, entry)} />
      </div>
    </div>
  )
}

export function NoticeRow({ entry, addFallback }: { entry: Entry; addFallback?: () => void }) {
  const tr = useTr()
  const text = tr(entry.data.text ?? '')
  if (entry.data.callSeconds !== undefined) {
    return (
      <HStack justify="center" gap={6}>
        <Icon name="waveform" size={10} color="var(--secondary)" />
        <Text size="footnote" tone="secondary" tabular>{text + callLength(entry.data.callSeconds)}</Text>
      </HStack>
    )
  }
  switch (entry.data.style) {
    case 'error':
      return (
        <HStack align="flex-start" gap={8} className="chat-notice-error">
          <Icon name="exclamationmark.triangle.fill" size={10} color="var(--danger)" />
          <VStack gap={10} align="flex-start">
            <Text size="footnote" tone="text" className="selectable">{text}</Text>
            {addFallback ? <Button kind="secondary" onClick={addFallback}>{tr('Add a fallback')}</Button> : null}
          </VStack>
        </HStack>
      )
    default:
      return <Text as="div" size="footnote" tone="secondary" className="chat-notice">{text}</Text>
  }
}

/** A message's reactions: the user's (clicking one takes it back), then the bots', each emoji with the avatars of the
 * bots that chose it. */
export function ReactionsRow({ entry }: { entry: Entry }) {
  const store = useStore()
  const mine = entry.data.reactions ?? []
  const theirs = groupBotReactions(entry.data.botReactions ?? [])
    .map(({ emoji, bots }) => ({ emoji, bots: bots.map((id) => store.bots.get(id)).filter((b): b is Bot => !!b) }))
    .filter((g) => g.bots.length)
  if (!mine.length && !theirs.length) return null
  return (
    <HStack align="stretch" gap={4}>
      {mine.map((emoji) => (
        <button key={emoji} className="press reaction-chip" title={`Remove ${emoji}`} aria-label={`Remove reaction ${emoji}`} onClick={() => store.react(entry, emoji)}>
          <Text size="body">{emoji}</Text>
        </button>
      ))}
      {theirs.map(({ emoji, bots }) => {
        const label = `${bots.map((b) => b.name).join(', ')} reacted ${emoji}`
        return (
          <HStack key={`bots-${emoji}`} gap={4} className="reaction-chip" title={label} aria-label={label} role="img">
            <Text size="body">{emoji}</Text>
            <HStack as="span" className="reaction-chip-bots">
              {bots.slice(0, 3).map((b) => (
                <span key={b.id}>
                  <BotAvatar bot={b} size={16} animated={false} />
                </span>
              ))}
            </HStack>
          </HStack>
        )
      })}
    </HStack>
  )
}

/** Compact actions beside the bubble: a reaction strip, reply, and more (copy, what it did). */
function MessageActions({ visible, className, reactions, reply, trace, copy }: {
  visible: boolean
  className: string
  reactions: ReactionPick
  reply?: () => void
  trace?: () => void
  /** What its Copy is (`copyItems`). */
  copy: MenuItem[]
}) {
  const [menu, setMenu] = useState<'reactions' | 'more' | null>(null)
  const smile = useRef<HTMLButtonElement>(null)
  const more = useRef<HTMLButtonElement>(null)
  const shown = visible || menu !== null
  return (
    <div className={`message-actions ${className} ${shown ? 'shown' : ''}`} aria-hidden={!shown}>
      <button ref={smile} className="message-action" title="Add reaction" aria-label="Add reaction" onClick={() => setMenu('reactions')}>
        <Icon name="face.smiling" size={14} />
      </button>
      {reply ? <IconButton title="Reply" icon="arrowshape.turn.up.left" size={26} onClick={reply} /> : null}
      <button ref={more} className="message-action" title="More message actions" aria-label="More message actions" onClick={() => setMenu('more')}>
        <Icon name="ellipsis" size={14} />
      </button>
      <AnchoredMenu open={menu === 'reactions'} onClose={() => setMenu(null)} anchor={smile} items={() => []} reactions={reactions} />
      <AnchoredMenu
        open={menu === 'more'}
        onClose={() => setMenu(null)}
        anchor={more}
        items={() => [...copy, ...(trace ? [{ title: 'Show what it did', icon: 'list.bullet', action: trace }] : [])]}
      />
    </div>
  )
}
