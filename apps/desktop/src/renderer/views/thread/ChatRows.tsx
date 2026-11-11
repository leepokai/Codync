import { FileDownloadCard } from './FileDownloadCard'
import { ArtifactCard } from '../library/ArtifactCard'
import { memo, useRef, useState } from 'react'
import { groupBotReactions, isGroup, isReplyable, isWorking, type Bot, type BotApp, type Entry, type ReplyQuote } from '@shared/models'
import { BotAvatar } from '../../components/Avatar'
import { ComputerCard } from '../../components/ComputerCard'
import { Button, IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { AnchoredMenu, useContextMenu, type MenuItem, type ReactionPick } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { messagePreview } from '../../lib/message-preview'
import { MemberQuestionCard } from './MemberQuestionCard'
import { useTr } from '../../lib/use-language'
import { avatarColor } from '../../lib/theme'
import { useStore as useLiveStore, useStoreRef as useStore } from '../../store/context'
import { QUICK_REACTIONS, type BotStore } from '../../store/bot-store'
import { AttachmentList } from './Attachments'
import { relativeTime } from './chat-items'
import { MarkdownText } from './Markdown'
import { quoteText } from './composer-edits'
import { PermissionCard } from './PermissionCard'
import { ConnectionRequestCard } from '../marketplace/ConnectionRequestCard'
import { AgentSwitchNotice, HandoffNotice } from './HandoffNotice'
import { VoiceReceipt } from './VoiceReceipt'

export function reactionPick(store: BotStore, entry: Entry): ReactionPick {
  return { emoji: QUICK_REACTIONS, chosen: entry.data.reactions ?? [], toggle: (e) => store.react(entry, e) }
}

/** Quotes a whole message into its conversation's draft (idea 34's fallback when there is no selection). */
export function quoteEntry(store: BotStore, entry: Entry) {
  const text = entry.data.text ?? ''
  if (!text) return
  const current = store.composerDrafts.get(entry.botId)
  const quoted = quoteText(text)
  store.composerDrafts.set(entry.botId, current ? `${current}\n${quoted}\n\n` : `${quoted}\n\n`)
}

/** Whether a message carries something to send on to another chat. */
function hasForwardable(entry: Entry) {
  return (
    (entry.kind === 'user' || entry.kind === 'agent') &&
    !!(entry.data.text?.trim() || entry.data.attachments?.length || entry.data.files?.length)
  )
}

/** One chat-visible entry, with what can be done to it. */
export const ChatRow = memo(function ChatRow({ entry, groupStart, chat, openTrace, onReply, onOpenFile, onForward, openAgent, openApp, openArtifact, openThread, onBranch }: {
  entry: Entry
  groupStart: boolean
  chat: Bot | null
  openTrace: () => void
  /** Quotes this message in the composer; absent where the computer can't take replies. */
  onReply?: (e: Entry) => void
  /** Opens a shared file in its own tab; absent where there is no room for it. */
  onOpenFile?: (e: Entry, file: import('@shared/models').SharedFile) => void
  /** Sends this message on to another chat; absent where the computer can't forward. */
  onForward?: (e: Entry) => void
  /** Opens a bot's Agent settings (a failure notice's "Add a fallback"). */
  openAgent?: (botId: string) => void
  /** Opens a sandboxed mini-app from its chat card. */
  openApp?: (app: BotApp) => void
  /** Opens an artifact card's artifact over the chat. */
  openArtifact?: (artifactId: string) => void
  /** Opens a thread from its root line. */
  openThread?: (rootId: string) => void
  /** Starts a thread branched from this chat message. */
  onBranch?: (e: Entry) => void
}) {
  const host = useStore()
  const reply = onReply && (isReplyable(entry) || entry.data.memberQuestion) ? () => onReply(entry) : undefined
  const forward = onForward && hasForwardable(entry) ? () => onForward(entry) : undefined
  // In the chat itself a reply can start a thread branched from it.
  const branch = onBranch && !entry.threadId && isReplyable(entry) ? () => onBranch(entry) : undefined
  switch (entry.kind) {
    case 'user':
      return (
        <VStack align="flex-end" gap={4} className={groupStart ? 'chat-row group-start' : 'chat-row'}>
          <UserBubble entry={entry} botWorking={!!chat && isWorking(chat)} reply={reply} forward={forward} branch={branch} />
          {entry.data.routineThread && openThread ? (
            <button className="thread-link press" onClick={() => openThread(entry.id)}>
              {entry.threadUnread ? <span className="thread-dot" aria-hidden /> : null}
              {entry.threadUnread ? `${entry.threadUnread} ` : ''}Open the thread
            </button>
          ) : null}
          <ReactionsRow entry={entry} />
          <ThreadChip entry={entry} openThread={openThread} />
        </VStack>
      )
    case 'agent':
      return (
        <VStack align="flex-start" gap={4} className={groupStart ? 'chat-row group-start' : 'chat-row'}>
          {chat && isGroup(chat) && groupStart ? <AuthorLabel botId={entry.data.author} /> : null}
          <AgentBubble entry={entry} openTrace={openTrace} reply={reply} forward={forward} openFile={onOpenFile} openArtifact={openArtifact} branch={branch} />
          <VStack gap={4} className={chat && isGroup(chat) ? 'chat-row-indent' : undefined}>
            <ReactionsRow entry={entry} />
            <ThreadChip entry={entry} openThread={openThread} />
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
      if (entry.data.memberQuestion) return <div className="chat-row group-start"><MemberQuestionCard entry={entry} reply={reply} /></div>
      if (entry.data.connectionRequest) return <div className="chat-row group-start"><ConnectionRequestCard entry={entry} /></div>
      if (entry.data.beliefLearning) return <div className="chat-row notice"><BeliefLearningNotice entry={entry} /></div>
      if (entry.data.handoff) return <div className="chat-row notice"><HandoffNotice entry={entry} card={entry.data.handoff} /></div>
      if (entry.data.takeoverRequest) return <div className="chat-row group-start"><TakeoverRequestNotice entry={entry} /></div>
      if (entry.data.pausedTurn) return <div className="chat-row group-start"><PausedTurnNotice entry={entry} /></div>
      if (entry.data.passkeyWall) return <div className="chat-row group-start"><PasskeyWallNotice entry={entry} /></div>
      if (entry.data.chiefIncident) return <div className="chat-row group-start"><ChiefIncidentNotice entry={entry} /></div>
      if (entry.data.botApp) return <div className="chat-row group-start"><BotAppNotice entry={entry} openApp={openApp} /></div>
      if (entry.data.agentSwitch) return <div className="chat-row notice"><AgentSwitchNotice entry={entry} carried={entry.data.agentSwitch.carried} /></div>
      return <div className="chat-row notice"><NoticeRow entry={entry} addFallback={entry.data.addFallback && openAgent ? () => openAgent(entry.data.author ?? entry.botId) : undefined} undo={entry.data.memoryUndo && !entry.data.memoryUndo.undone ? () => void host.undoMemoryChange(entry) : undefined} /></div>
  }
})

function BeliefLearningNotice({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  const learning = entry.data.beliefLearning
  if (!learning) return null
  return <div className="handoff-card">
    <Text size="footnote">{entry.data.text}</Text>
    <div key={learning.status} className="fade-in">
      {learning.status === 'active'
        ? <HStack gap={8}>{learning.beliefState === 'confirmed'
          ? <Button kind="secondary" disabled={store.answering.has(entry.id)} onClick={() => store.changeBelief(entry, 'forgetBelief')}>Forget</Button>
          : <Button kind="secondary" disabled={store.answering.has(entry.id)} onClick={() => store.changeBelief(entry, 'confirmBelief')}>Confirm</Button>}
          <Button kind="secondary" disabled={store.answering.has(entry.id)} onClick={() => store.undoBeliefObservation(entry)}>Undo learning</Button></HStack>
        : learning.status === 'confirmed'
          ? <HStack gap={8}><Text size="footnote" tone="secondary">Confirmed</Text><Button kind="secondary" disabled={store.answering.has(entry.id)} onClick={() => store.changeBelief(entry, 'forgetBelief')}>Forget</Button></HStack>
          : learning.status === 'forgotten'
            ? <HStack gap={8}><Text size="footnote" tone="secondary">Forgotten</Text><Button kind="secondary" disabled={store.answering.has(entry.id)} onClick={() => store.changeBelief(entry, 'restoreBelief')}>Restore</Button></HStack>
            : <Text size="footnote" tone="secondary">Undone</Text>}
    </div>
  </div>
}

/** Sandboxed mini-app card published by the bot (Ideas 1–7). */
function BotAppNotice({ entry, openApp }: { entry: Entry; openApp?: (app: BotApp) => void }) {
  const app = entry.data.botApp
  if (!app) return null
  return (
    <VStack gap={8} className="bubble agent">
      <HStack gap={8} align="center">
        <Icon name="macwindow" size={14} color="var(--accent)" />
        <Text weight="semibold">{app.title}</Text>
        <span className="t-tab-badge">v{app.version}</span>
      </HStack>
      {app.description ? <Text size="footnote" tone="secondary">{app.description}</Text> : null}
      <HStack gap={8}>
        {openApp ? (
          <Button kind="primary" onClick={() => openApp(app)}>
            Open App
          </Button>
        ) : null}
        {app.url ? (
          <Button kind="secondary" onClick={() => window.codync.app.copy(app.url!)}>
            Copy Link
          </Button>
        ) : null}
      </HStack>
    </VStack>
  )
}

/** Ideas 13, 19 & 20: Human takeover request rendered with ComputerCard and TakeoverModal. */
function TakeoverRequestNotice({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  const req = entry.data.takeoverRequest
  if (!req) return null
  const resolved = req.status === 'resolved' || req.status === 'cancelled' || req.status === 'expired'
  return (
    <ComputerCard
      frame={{
        url: req.url ?? undefined,
        title: req.reason,
        action: req.status === 'expired' ? 'Expired' : resolved ? 'Completed' : 'Action needed',
        active: !resolved,
      }}
      onFinishTakeover={() =>
        store.send('I have completed the requested step in the browser. Please continue.', entry.botId, {
          thread: entry.threadId ?? null,
        })
      }
    />
  )
}

/** A turn that stopped half done: said so, with one tap to go on (a few times in a row, then it asks for words). */
function PausedTurnNotice({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  const tr = useTr()
  const [error, setError] = useState<string | null>(null)
  const paused = entry.data.pausedTurn
  if (!paused) return null
  const open = paused.status === 'waiting'
  return (
    <VStack gap={8} className="bubble agent">
      <HStack gap={8}>
        <Icon name="pause.circle.fill" size={14} color="var(--warning, #ff9f0a)" />
        <Text weight="semibold" tone="attention">{tr(paused.verdict === 'truncated' ? 'Answer cut short' : 'Paused halfway')}</Text>
      </HStack>
      <Text size="footnote" tone="secondary">{entry.data.text}</Text>
      {open ? (
        <>
          {!paused.canContinue ? <Text size="caption" tone="tertiary">{tr('Say what remains in a message.')}</Text> : null}
          <HStack gap={8}>
            {paused.canContinue ? (
              <Button kind="primary" onClick={() => void store.continueTurn(entry).then(setError)}>{tr('Continue')}</Button>
            ) : null}
            <Button kind="secondary" onClick={() => void store.cancelTask(entry).then(setError)}>{tr('Cancel task')}</Button>
          </HStack>
        </>
      ) : (
        <Text size="caption" tone="tertiary">{tr(paused.status === 'closed' ? 'Cancelled' : 'Continued')}</Text>
      )}
      {error ? <Text size="caption" tone="danger">{error}</Text> : null}
    </VStack>
  )
}

/** Interactive Passkey / 2FA / Captcha assistance card from CDP browser detector (INIC-02). */
function PasskeyWallNotice({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  const wall = entry.data.passkeyWall
  if (!wall) return null
  const resolved = wall.status === 'resolved' || wall.status === 'cancelled'
  return (
    <VStack gap={8} className="bubble agent">
      <HStack gap={8}>
        <Icon name="person.badge.key.fill" size={14} color="var(--warning, #ff9f0a)" />
        <Text weight="semibold" tone="attention">
          Authentication Required ({wall.kind.toUpperCase()})
        </Text>
      </HStack>
      <Text size="footnote" tone="secondary">{wall.reason} · {wall.url}</Text>
      {!resolved ? (
        <HStack gap={8}>
          <Button kind="primary" onClick={() => store.send('I have completed the authentication in the browser. Please continue.', entry.botId, { thread: entry.threadId ?? null })}>
            Continue
          </Button>
          <Button kind="secondary" onClick={() => store.stop(entry.botId)}>
            Cancel
          </Button>
        </HStack>
      ) : (
        <Text size="caption" tone="tertiary">{wall.status === 'resolved' ? 'Authentication completed' : 'Cancelled'}</Text>
      )}
    </VStack>
  )
}

/** Chief of Staff automatic intervention / escalation card (INIC-04). */
function ChiefIncidentNotice({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  const incident = entry.data.chiefIncident
  if (!incident) return null
  const isEscalated = incident.status === 'escalated' || incident.retryAttempt >= 2
  return (
    <VStack gap={8} className="bubble agent">
      <HStack gap={8}>
        <Icon
          name="shield.lefthalf.filled"
          size={14}
          color={isEscalated ? 'var(--danger)' : 'var(--warning, #ff9f0a)'}
        />
        <Text weight="semibold" tone={isEscalated ? 'danger' : 'attention'}>
          Chief of Staff · {isEscalated ? 'Escalated Incident' : `Intervention (Attempt ${incident.retryAttempt}/2)`}
        </Text>
      </HStack>
      <Text size="footnote" tone="secondary">
        Worker: {store.authorName(incident.workerBotId)} · Reason: {incident.reason}
      </Text>
      <Text size="footnote">{incident.summary}</Text>
      {isEscalated ? (
        <HStack gap={8}>
          <Button kind="secondary" onClick={() => store.send(`Retry assignment for ${store.authorName(incident.workerBotId)}`, entry.botId, { thread: entry.threadId ?? null })}>
            Retry manually
          </Button>
          <Button kind="secondary" onClick={() => store.stop(incident.workerBotId)}>
            Stop worker
          </Button>
        </HStack>
      ) : null}
    </VStack>
  )
}

/** Follows the store: the card shows a spinner while its answer is on its way. */
function LivePermission({ entry }: { entry: Entry }) {
  const store = useLiveStore()
  return <PermissionCard entry={entry} hostName={store.hostName} answering={store.answering.get(entry.id) ?? null} respond={(o, rule) => store.respond(entry, o, rule)} propose={() => store.proposeRule(entry)} />
}

/** A message's thread, under it: who replied, how many, and how recently (or how many are new). */
function ThreadChip({ entry, openThread }: { entry: Entry; openThread?: (rootId: string) => void }) {
  const store = useStore()
  const summary = entry.data.thread
  if (!summary || summary.count < 1 || !openThread) return null
  const authors = (summary.authors ?? []).flatMap((id) => {
    const bot = store.bots.get(id)
    return bot ? [bot] : []
  })
  return (
    <button
      className="thread-chip press"
      onClick={() => openThread(entry.id)}
      aria-label={`View thread, ${summary.count} ${summary.count === 1 ? 'reply' : 'replies'}${summary.unread ? `, ${summary.unread} new` : ''}`}
    >
      {authors.length ? (
        <span className="thread-chip-authors">
          {authors.slice(0, 3).map((bot) => (
            <BotAvatar key={bot.id} bot={bot} size={18} animated={false} />
          ))}
        </span>
      ) : null}
      <span className="thread-chip-count">{summary.count === 1 ? '1 reply' : `${summary.count} replies`}</span>
      {summary.unread > 0 ? (
        <span className="thread-chip-new">{summary.unread} new</span>
      ) : summary.lastAt > 0 ? (
        <span className="thread-chip-when">{relativeTime.day(summary.lastAt)}</span>
      ) : null}
      <Icon name="chevron.right" size={10} color="var(--tertiary)" />
    </button>
  )
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

/** Said as forwarded from another chat: the author on top of the copy. */
function ForwardedRow({ forwarded }: { forwarded: NonNullable<Entry['data']['forwardedFrom']> }) {
  const tr = useTr()
  return (
    <HStack gap={6} className="bubble-quote">
      <Icon name="arrowshape.turn.up.right" size={10} color="var(--secondary)" />
      <Text size="footnote" tone="secondary" truncate grow>{`${tr('Forwarded from')} ${forwarded.author}`}</Text>
    </HStack>
  )
}

/** A message's text with a right-click menu (with quick reactions on top). */
export function Bubble({ className, items, reactions, children }: { className: string; items: () => MenuItem[]; reactions?: ReactionPick; children: React.ReactNode }) {
  const menu = useContextMenu()
  return (
    <>
      <div className={className} dir="auto" onContextMenu={menu.onContextMenu}>
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

export function UserBubble({ entry, botWorking, reply, forward, branch }: { entry: Entry; botWorking: boolean; reply?: () => void; forward?: () => void; branch?: () => void }) {
  const store = useStore()
  const [hovering, setHovering] = useState(false)
  const status = entry.data.status
  const copy = () => entry.data.text && window.codync.app.copy(entry.data.text)
  return (
    <div className="message user" title={fullDate.format(entry.createdAt)} onMouseEnter={() => setHovering(true)} onMouseLeave={() => setHovering(false)}>
      <div className="message-body">
        {entry.data.attachments?.length ? <AttachmentList attachments={entry.data.attachments} botId={entry.botId} /> : null}
        {entry.data.text || entry.data.replyTo ? (
          <Bubble
            className="bubble user selectable"
            reactions={reactionPick(store, entry)}
            items={() => [
              { title: 'Copy', icon: 'square.on.square', action: copy },
            { title: 'Quote', icon: 'text.quote', action: () => quoteEntry(store, entry) },
              { title: 'Bookmark', icon: 'bookmark', action: () => void store.saveBookmark(entry.botId, entry.id, entry.data.text ?? '', '', entry.data.author ?? 'user', entry.threadId ?? null) },
              ...(reply ? [{ title: 'Reply', icon: 'arrowshape.turn.up.left', action: reply }] : []),
              ...(branch ? [{ title: 'Branch from here', icon: 'arrow.triangle.branch', action: branch }] : []),
              ...(forward ? [{ title: 'Send to a channel…', icon: 'arrowshape.turn.up.right', action: forward }] : []),
              { title: 'Delete', icon: 'trash', destructive: true, action: () => void store.deleteEntry(entry.id) },
            ]}
          >
            {entry.data.forwardedFrom ? <ForwardedRow forwarded={entry.data.forwardedFrom} /> : null}
            {entry.data.replyTo ? <QuoteRow quote={entry.data.replyTo} /> : null}
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
  const tr = useTr()
  const store = useStore()
  switch (entry.data.status) {
    case 'sending':
      return <Text size="caption2" tone="tertiary">{tr(`Sending…`)}</Text>
    case 'queued':
      return <Text size="caption2" tone="tertiary">{botWorking ? tr(`Queued until this response finishes`) : tr(`Queued`)}</Text>
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

export function AgentBubble({ entry, openTrace, reply, forward, openFile, openArtifact, branch }: { entry: Entry; openTrace: () => void; reply?: () => void; forward?: () => void; openFile?: (e: Entry, file: import('@shared/models').SharedFile) => void; openArtifact?: (artifactId: string) => void; branch?: () => void }) {
  const store = useStore()
  const [hovering, setHovering] = useState(false)
  const copy = () => entry.data.text && window.codync.app.copy(entry.data.text)
  return (
    <div className="message agent" title={fullDate.format(entry.createdAt)} onMouseEnter={() => setHovering(true)} onMouseLeave={() => setHovering(false)}>
      <div className="message-body">
        <Bubble
          className={entry.data.files?.length || entry.data.artifact ? 'file-message' : 'bubble agent'}
          reactions={reactionPick(store, entry)}
          items={() => [
            { title: 'Copy', icon: 'square.on.square', action: copy },
              { title: 'Quote', icon: 'text.quote', action: () => quoteEntry(store, entry) },
              ...(reply ? [{ title: 'Reply', icon: 'arrowshape.turn.up.left', action: reply }] : []),
              ...(branch ? [{ title: 'Branch from here', icon: 'arrow.triangle.branch', action: branch }] : []),
              ...(forward ? [{ title: 'Send to a channel…', icon: 'arrowshape.turn.up.right', action: forward }] : []),
              { title: 'Delete', icon: 'trash', destructive: true, action: () => void store.deleteEntry(entry.id) },
            { title: 'Show what it did', icon: 'list.bullet', action: openTrace },
          ]}
        >
          {entry.data.forwardedFrom ? <ForwardedRow forwarded={entry.data.forwardedFrom} /> : null}
          {entry.data.artifact ? (
            <ArtifactCard botId={entry.botId} card={entry.data.artifact} open={() => openArtifact?.(entry.data.artifact!.id)} />
          ) : entry.data.files?.length ? (
            <div className="file-message-cards">{entry.data.files.map((file) => <FileDownloadCard key={file.id} entry={entry} file={file} onOpen={openFile ? () => openFile(entry, file) : undefined} />)}</div>
          ) : (
            <MarkdownText text={entry.data.text ?? ''} />
          )}
        </Bubble>
        <MessageActions className="trailing" visible={hovering} reactions={reactionPick(store, entry)} reply={reply} trace={openTrace} copy={copy} />
      </div>
    </div>
  )
}

export function NoticeRow({ entry, addFallback, undo }: { entry: Entry; addFallback?: () => void; undo?: () => void }) {
  if (window.codync.platform === 'win32' && entry.data.source === 'voice_receipt') return <VoiceReceipt entry={entry} />
  const text = entry.data.text ?? ''
  if (entry.data.callSeconds !== undefined) {
    const s = entry.data.callSeconds
    return (
      <HStack justify="center" gap={6}>
        <Icon name="waveform" size={10} color="var(--secondary)" />
        <Text size="footnote" tone="secondary" tabular>{`${text} · ${String(Math.floor(s / 60)).padStart(2, '0')}:${String(s % 60).padStart(2, '0')}`}</Text>
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
            {addFallback ? <Button kind="secondary" onClick={addFallback}>Add a fallback</Button> : null}
            {undo ? <Button kind="secondary" onClick={undo}>Undo</Button> : null}
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
  copy: () => void
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
        items={() => [{ title: 'Copy', icon: 'square.on.square', action: copy }, ...(trace ? [{ title: 'Show what it did', icon: 'list.bullet', action: trace }] : [])]}
      />
    </div>
  )
}
