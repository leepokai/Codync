import { memo, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { draftOf, isGroup, isWorking, isWorkingIn, type Bot, type BotDraft, type Entry, type ReplyQuote } from '@shared/models'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Sheet } from '../../components/Overlay'
import { HStack, IconSlot } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { connectedThrough } from '../../lib/relay-copy'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { UpdateNeededCard } from '../UpdateNeededCard'
import { buildChat, type ChatItem } from './chat-items'
import { ChatLog, useDockHeight } from './ChatLog'
import { BotConversationView, BotMessageRow } from './BotConversation'
import { CallConversationView, CallLineRow } from '../call/CallRows'
import { hidingCalls } from '@shared/calls'
import { devServerSummary } from '@shared/dev-servers'
import { ChatRow, TimeSeparator } from './ChatRows'
import { RequestRun } from './PermissionCard'
import { DevServerTitle } from './DevServersSection'
import { WorkingTransition } from './WorkingTransition'
import { RoutineNotice } from './RoutineNotice'
import { Composer } from './Composer'
import { QueuedMessages } from './QueuedMessages'
import { WorkCapsule } from './WorkCapsule'
import { fill } from '@shared/fill'
import { outsideQueue } from './queued-messages'
import { ChatSidePanel, useSidePanelWidth } from './ChatSidePanel'
import { CONVERSATION_MIN } from './side-panel-width'
import { WindowBarSlots } from '../WindowBar'
import { DetailsPanel, showPanelView, type DetailsTab } from './DetailsPanel'
import { PanelBarButtons } from './PanelBarButtons'
import { TraceView } from './TraceView'
import { workingSteps } from './trace-turns'
import { useReading } from './reading'
import { GroupEditorView } from '../bots/GroupEditorView'
import { BotTemplateView } from '../bots/BotTemplateView'
import { BotEditorView } from '../bots/BotEditorView'
import { FormSheet } from '../marketplace/pieces'
import { CallView } from '../call/CallView'
import { ArtifactView } from '../library/ArtifactView'
import './chat.css'
import './composer.css'
import './chat-controls.css'

/**
 * One endless conversation with a bot or a group chat. Only deliberate messages show here; tool
 * calls and thinking live in the "Full conversation" sheet. Replying to a message quotes it in the composer.
 */
export const ChatView = memo(function ChatView({ botId }: { botId: string }) {
  const tr = useTr()
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const [showTrace, setShowTrace] = useState(false)
  const [replyTo, setReplyTo] = useState<ReplyQuote | null>(null)
  const cancelReply = useCallback(() => setReplyTo(null), [])
  const [botChat, setBotChat] = useState<string | null>(null)
  /** The voice call whose conversation is open (its line's entry id). */
  const [callLine, setCallLine] = useState<string | null>(null)
  const [editingGroup, setEditingGroup] = useState(false)
  const [templateDraft, setTemplateDraft] = useState<BotDraft | null>(null)
  const [showSettings, setShowSettings] = useState(false)
  // The docked panel covers the conversation.
  const [panelFilled, setPanelFilled] = useState(false)
  const [calling, setCalling] = useState(false)
  const [callSpeaking, setCallSpeaking] = useState(false)
  const interruptCall = useRef<(() => void) | null>(null)
  const [routineId, setRoutineId] = useState<string | null>(null)
  // A group member's settings, opened from its notice.
  const [editingMember, setEditingMember] = useState<string | null>(null)
  const [routineRequest, setRoutineRequest] = useState(0)
  // The artifact shown over the chat (a card or the Library opened it).
  const [artifactId, setArtifactId] = useState<string | null>(null)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  const [detailsWidth, setDetailsWidth] = usePref(prefs.detailsPanelWidth)
  // Beside the conversation whenever both fit, the conversation narrowing to make room; otherwise it covers it.
  const panel = useSidePanelWidth(root, CONVERSATION_MIN, detailsWidth)
  useReading(store, botId)
  // The chat window remounts this per conversation (`key`), so each one opened counts.
  useScreenViewed('chat')

  useDockHeight(root, dock)

  const panelOpen = showSettings
  const covers = !panel.beside
  const bar = useContext(WindowBarSlots)
  const toggleDetails = useCallback(() => setShowSettings((s) => !s), [])
  const devServers = devServerSummary(bot?.devServers ?? [])
  // The title's port leads to its row, whichever page the inspector was left on.
  const openFromTitle = useCallback(() => {
    if (devServers.kind !== 'none') showPanelView('details')
    toggleDetails()
  }, [devServers.kind, toggleDetails])

  useEffect(
    () =>
      window.codync.app.onCommand((c) => {
        if (c.kind === 'toggleDetails') toggleDetails()
      }),
    [toggleDetails],
  )

  const openTrace = useCallback(() => setShowTrace(true), [])
  const openArtifact = useCallback((id: string) => {
    setArtifactId(id)
    setPanelFilled(false)
    if (covers) setShowSettings(false)
  }, [covers])

  const showDetails = useCallback((tab: DetailsTab) => {
    showPanelView(tab)
    setShowSettings(true)
  }, [])

  const presentRoutine = useCallback((id: string | null) => {
    setRoutineId(id)
    setRoutineRequest((r) => r + 1)
    showDetails('details')
  }, [showDetails])

  // This bot's Agent card is on the Settings tab; a group member's is in its settings.
  const presentAgent = useCallback((id: string) => (id === botId ? showDetails('settings') : setEditingMember(id)), [botId, showDetails])
  // The capsule opens what the bot does.
  const openWork = useCallback(() => showDetails('details'), [showDetails])
  // The chat beside the panel at the card a task that needs the user waits on (the one its row reads).
  const [reveal, setReveal] = useState<Reveal | null>(null)
  const openCard = useCallback((taskId: string) => {
    if (covers) setShowSettings(false)
    setReveal({ taskId })
  }, [covers])
  const member = editingMember ? store.bots.get(editingMember) : undefined

  const shareBot = () => {
    if (bot) setTemplateDraft(draftOf(bot))
  }
  const details = (
    <DetailsPanel
      key={routineRequest}
      botId={botId}
      routineId={routineId}
      docked={bar.end !== null}
      editGroup={() => setEditingGroup(true)}
      openArtifact={openArtifact}
      openCard={openCard}
      share={shareBot}
      close={() => setShowSettings(false)}
    />
  )

  return (
    <div className="chat-view" ref={root}>
      {/* Covered by the maximized panel, it leaves the keyboard's reach too. */}
      <div className="conversation" inert={panelOpen && (panelFilled || covers)}>
        <div className="top-chrome">
          <TopFade />
          {/* The window's bar holds the bot, what runs it, and the panel's toggle with the open panel's own buttons
              before it; nothing while another page covers the conversation. */}
          {bar.end ? createPortal(<PanelBarButtons open={panelOpen && !!bot && !isGroup(bot)} filled={panelFilled} setFilled={covers ? undefined : setPanelFilled} share={shareBot} toggle={toggleDetails} />, bar.end) : null}
          {bar.title ? createPortal(<>
          <button className={`title-pill ${calling ? 'hidden-by-call' : ''}`} inert={calling} onClick={openFromTitle} aria-label={tr('{name}, view details').replaceAll('{name}', () => bot?.name ?? tr('Conversation'))} title={tr('View conversation details')}>
            <HStack as="span" gap={7}>
              {bot ? isGroup(bot) ? <GroupAvatar members={store.members(bot)} size={22} /> : <BotAvatar bot={bot} size={22} /> : null}
              <Text size="body" weight="medium" tone="text" truncate>{bot?.name ?? ''}</Text>
              <DevServerTitle summary={devServers} />
              {/* Grok Bot's hint on hover: the pill opens the details. */}
              <span className="title-pill-chevron">
                <Icon name="chevron.right" size={10} weight="semibold" color="var(--secondary)" />
              </span>
            </HStack>
            {store.shownConnection.kind !== 'online' || store.mismatch ? <ConnectionSubtitle /> : null}
          </button>
          </>, bar.title) : null}
        </div>
        {/* An update card takes the composer's place, so there's nothing to reply with. */}
        <Transcript botId={botId} reveal={reveal} openTrace={openTrace} onReply={store.mismatch ? undefined : setReplyTo} openRoutine={presentRoutine} openAgent={presentAgent} openBotChat={setBotChat} openArtifact={openArtifact} openCall={setCallLine} />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          {store.mismatch ? (
            <div className="composer-update-needed">
              <UpdateNeededCard />
            </div>
          ) : artifactId ? null : (
            // The artifact over the chat has its own box (two would both take the voice shortcut).
            <>
              <WorkCapsule botId={botId} open={openWork} />
              <QueuedMessages botId={botId} />
              <Composer
                botId={botId}
                reply={replyTo}
                cancelReply={cancelReply}
                onCall={!calling && bot ? () => setCalling(true) : null}
                onInterrupt={callSpeaking ? () => interruptCall.current?.() : null}
              />
            </>
          )}
        </div>
        {calling ? (
          <div className="call-overlay">
            <CallView botId={botId} onSpeaking={setCallSpeaking} interrupt={interruptCall} onEnd={() => setCalling(false)} />
          </div>
        ) : null}
        {artifactId ? <ArtifactView botId={botId} artifactId={artifactId} close={() => setArtifactId(null)} /> : null}
      </div>
      <ChatSidePanel
        open={panelOpen}
        filled={panelFilled || covers}
        width={panel.width}
        max={panel.max}
        onResize={(w) => setDetailsWidth(panel.fit(w))}
      >
        {details}
      </ChatSidePanel>
      <Sheet open={botChat !== null} onClose={() => setBotChat(null)} width={620} height={560}>
        {botChat ? <BotConversationView botId={botId} peerId={botChat} /> : null}
      </Sheet>
      <Sheet open={callLine !== null} onClose={() => setCallLine(null)} width={620} height={560}>
        {callLine ? <CallConversationView botId={botId} lineId={callLine} /> : null}
      </Sheet>
      <Sheet open={showTrace} onClose={() => setShowTrace(false)} width={620} height={560}>
        <TraceView botId={botId} />
      </Sheet>
      <Sheet open={templateDraft !== null} onClose={() => setTemplateDraft(null)}>
        {templateDraft ? <BotTemplateView draft={templateDraft} /> : null}
      </Sheet>
      <Sheet open={editingGroup} onClose={() => setEditingGroup(false)} width={420} height={560}>
        {bot ? <GroupEditorView group={bot} /> : null}
      </Sheet>
      <FormSheet open={!!member} onClose={() => setEditingMember(null)} width={520} height={680} fixed>
        {member ? <BotEditorView key={member.id} draft={draftOf(member)} /> : null}
      </FormSheet>
    </div>
  )
})

/** The top edge of the chat: messages fade out gradually as they scroll under the title pill. */
function TopFade() {
  return <div className="top-fade" />
}

function ConnectionSubtitle() {
  const store = useStore()
  const tr = useTr()
  const connecting = store.shownConnection.kind === 'connecting'
  const symbol = store.connection.kind !== 'online' ? 'wifi.slash' : store.hostRoute === 'relay' ? 'cloud' : store.hostRoute === 'direct' ? 'wifi' : store.hostRoute === 'loopback' ? 'desktopcomputer' : 'network'
  const description = (() => {
    if (store.isWaking) return 'Waking the computer…'
    if (store.isAsleep) return 'Sleeping · wakes when you message'
    switch (store.shownConnection.kind) {
      case 'connecting':
        return 'Connecting'
      case 'computerOffline':
      case 'offline':
        return 'Offline'
      case 'unauthorized':
        return 'No access'
      case 'unpaired':
        return 'Not paired'
    }
    if (store.isCloudUpdating) return 'Updating…'
    if (store.mismatch) return 'Needs update'
    return store.hostRoute === 'relay' ? connectedThrough(store.computer.cloud) : store.hostRoute === 'direct' ? tr('Connected over Wi-Fi or Tailscale') : store.hostRoute === 'loopback' ? tr('Connected locally') : 'Connected'
  })()
  const spoken = tr(description)
  return (
    <HStack as="span" gap={6} className="title-pill-subtitle" title={`${store.hostName} · ${spoken}`} aria-label={`${store.hostName}, ${spoken}`}>
      <IconSlot width={12}>{connecting ? <Spinner size={8} /> : <Icon name={symbol} size={8} weight="medium" />}</IconSlot>
      <Text size="caption" weight="medium" truncate>{store.isAsleep ? spoken : store.connectionLabel}</Text>
    </HStack>
  )
}

/**
 * The bot's chat in a panel beside one of its threads: its messages and its message box, with its sheets (Full
 * conversation, a talk with another bot, a call's conversation). What opens elsewhere (a routine, an agent's settings)
 * waits for the chat itself.
 */
export function SideChat({ botId, openArtifact }: { botId: string; openArtifact: (artifactId: string) => void }) {
  const store = useStore()
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)
  useScreenViewed('side_chat')
  const [replyTo, setReplyTo] = useState<ReplyQuote | null>(null)
  const [trace, setTrace] = useState(false)
  const [peer, setPeer] = useState<string | null>(null)
  const [line, setLine] = useState<string | null>(null)
  const nothing = useCallback(() => {}, [])
  return (
    <div className="chat-view side-chat">
      <div className="conversation" ref={root}>
        <Transcript botId={botId} reveal={null} openTrace={() => setTrace(true)} onReply={store.mismatch ? undefined : setReplyTo} openRoutine={nothing} openAgent={nothing} openBotChat={setPeer} openArtifact={openArtifact} openCall={setLine} />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          {store.mismatch ? null : <Composer botId={botId} reply={replyTo} cancelReply={() => setReplyTo(null)} />}
        </div>
      </div>
      <Sheet open={trace} onClose={() => setTrace(false)} width={620} height={560}><TraceView botId={botId} /></Sheet>
      <Sheet open={peer !== null} onClose={() => setPeer(null)} width={620} height={560}>{peer ? <BotConversationView botId={botId} peerId={peer} /> : null}</Sheet>
      <Sheet open={line !== null} onClose={() => setLine(null)} width={620} height={560}>{line ? <CallConversationView botId={botId} lineId={line} /> : null}</Sheet>
    </div>
  )
}

/** Where the chat is brought to: a task's card. */
type Reveal = { taskId: string }

/** The chat's messages: every loaded one (`ChatLog`), without what voice calls carried and what waits for the bot. */
const Transcript = memo(function Transcript({ botId, reveal, openTrace, onReply, openRoutine, openAgent, openBotChat, openArtifact, openCall }: { botId: string; reveal: Reveal | null; openTrace: () => void; onReply?: (quote: ReplyQuote) => void; openRoutine: (id: string | null) => void; openAgent: (botId: string) => void; openBotChat: (peerId: string) => void; openArtifact: (artifactId: string) => void; openCall: (lineId: string) => void }) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const live = !!bot && isWorkingIn(bot, botId) && !store.isOffline
  // Rebuilt only when this chat changes: the store replaces a chat's list on every change.
  const entries = store.allEntries(botId)
  // What voice calls carried shows in each call's conversation, not in the chat; what waits for the bot, in the queue.
  const working = !!bot && isWorking(bot)
  const chatOf = useCallback((list: Entry[]) => buildChat(outsideQueue(hidingCalls(list), working)), [working])
  const all = useMemo(() => chatOf(entries), [chatOf, entries])
  const steps = useMemo(() => workingSteps(entries), [entries])
  // `reveal`: a task's card loads older pages until the chat has it.
  const [revealed, setRevealed] = useState<{ entryId: string } | null>(null)
  useEffect(() => {
    if (!reveal) return
    let current = true
    void (async () => {
      const entryId = (await store.cardOf(botId, reveal.taskId))?.id
      if (current && entryId) setRevealed({ entryId })
    })()
    return () => {
      current = false
    }
  }, [reveal])
  return (
    <ChatLog
      items={all}
      fresh={() => chatOf(store.allEntries(botId))}
      canLoadOlder={store.canLoadOlder(botId)}
      loadOlder={() => store.loadOlder(botId)}
      revealed={revealed}
      intro={bot ? isGroup(bot) ? <GroupIntroCard group={bot} /> : <IntroCard bot={bot} /> : null}
      row={(item) => <Row item={item} chat={bot} openTrace={openTrace} onReply={onReply} openRoutine={openRoutine} openAgent={openAgent} openBotChat={openBotChat} openArtifact={openArtifact} openCall={openCall} />}
      working={bot ? <WorkingTransition bot={bot} visible={live} steps={steps} /> : null}
    />
  )
})

function Row({ item, chat, openTrace, onReply, openRoutine, openAgent, openBotChat, openArtifact, openCall }: { item: ChatItem; chat: Bot | null; openTrace: () => void; onReply?: (quote: ReplyQuote) => void; openRoutine: (id: string | null) => void; openAgent: (botId: string) => void; openBotChat: (peerId: string) => void; openArtifact: (artifactId: string) => void; openCall: (lineId: string) => void }) {
  if (item.kind === 'separator') {
    return <TimeSeparator date={item.date} />
  }
  if (item.kind === 'exchanges') return <BotMessageRow exchanges={item.exchanges} open={openBotChat} />
  if (item.kind === 'requests') return <div className="chat-row group-start"><RequestRun entries={item.entries} row={(e) => <ChatRow entry={e} groupStart={false} chat={chat} />} /></div>
  const e = item.entry
  if (e.kind === 'notice' && e.data.routineId) return <RoutineNotice entry={e} open={() => openRoutine(e.data.routineId!)} />
  if (e.kind === 'notice' && e.data.call) return <div className="chat-row notice"><CallLineRow entry={e} open={() => openCall(e.id)} /></div>

  return <ChatRow entry={e} groupStart={item.groupStart} chat={chat} openTrace={openTrace} onReply={onReply} openAgent={openAgent} openArtifact={openArtifact} />
}

/** The start of an empty group chat: who's in it and how the room works. */
function GroupIntroCard({ group }: { group: Bot }) {
  const tr = useTr()
  const store = useStore()
  const members = store.members(group)
  const names = members.map((m) => m.name)
  return (
    <div className="intro-card">
      <GroupAvatar members={members} size={72} />
      <Text as="div" size="title2" weight="semibold" tone="text">{group.name}</Text>
      {group.name !== names.join(', ') ? <Text as="div" size="subheadline" tone="secondary">{names.join(' · ')}</Text> : null}
      {group.description ? <Text as="div" size="subheadline" tone="secondary">{group.description}</Text> : null}
      <Text as="div" size="callout" tone="secondary" className="intro-card-hint balanced">{tr(`Everyone answers in turn. @mention a bot to ask just that one.`)}</Text>
    </div>
  )
}

function IntroCard({ bot }: { bot: Bot }) {
  const store = useStore()
  const tr = useTr()
  const where = bot.managedWorkspace
    ? `${store.backendName(bot.backend)} · ${tr('Own folder')}`
    : fill(tr('{agent} in {path}'), { agent: store.backendName(bot.backend), path: bot.cwd })
  return (
    <div className="intro-card">
      <BotAvatar bot={bot} size={72} />
      <Text as="div" size="title2" weight="semibold" tone="text">{bot.name}</Text>
      <Text as="div" size="callout" tone="secondary" className="intro-card-path">
        {where}
      </Text>
      {bot.description ? <Text as="div" size="subheadline" tone="secondary">{bot.description}</Text> : null}
      <Text as="div" size="callout" tone="secondary" className="intro-card-hint">{tr(`Tell it what you need. You'll get a notification when it's done or needs you.`)}</Text>
    </div>
  )
}
