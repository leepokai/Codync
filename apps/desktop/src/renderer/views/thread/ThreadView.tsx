import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import { draftOf, isGroup, isWorkingIn, type Bot, type BotDraft, type Entry } from '@shared/models'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Sheet, usePresence } from '../../components/Overlay'
import { HStack, IconSlot, Spacer } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { connectedThrough } from '../../lib/relay-copy'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import { useStore } from '../../store/context'
import { UpdateNeededCard } from '../UpdateNeededCard'
import { buildChat, rowIn, useArrivals, type ChatItem } from './chat-items'
import { BotConversationView, BotMessageRow } from './BotConversation'
import { CallConversationView, CallLineRow } from '../call/CallRows'
import { hidingCalls } from '@shared/calls'
import { ChatRow, TimeSeparator } from './ChatRows'
import { WorkingTransition } from './WorkingTransition'
import { RoutineNotice } from './RoutineNotice'
import { Composer } from './Composer'
import { ChatSidePanel } from './ChatSidePanel'
import { DetailsPanel, type DetailsTab } from './DetailsPanel'
import { TraceView } from './TraceView'
import { workingSteps } from './trace-turns'
import { useReading } from './reading'
import { GroupEditorView } from '../bots/GroupEditorView'
import { BotTemplateView } from '../bots/BotTemplateView'
import { BotEditorView } from '../bots/BotEditorView'
import { FormSheet } from '../marketplace/pieces'
import { CallView } from '../call/CallView'
import { MacChatButton } from './MacChatButton'
import { ArtifactView } from '../library/ArtifactView'
import './thread.css'
import './composer.css'
import './chat-controls.css'

const WIDE = 740

/**
 * One endless conversation with a bot or a group chat. Only deliberate messages show here; tool
 * calls and thinking live in the "Full conversation" sheet. Replying to a message quotes it in the composer.
 */
export const ThreadView = memo(function ThreadView({ botId }: { botId: string }) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const [showTrace, setShowTrace] = useState(false)
  const [replyTo, setReplyTo] = useState<Entry | null>(null)
  const cancelReply = useCallback(() => setReplyTo(null), [])
  const [botChat, setBotChat] = useState<string | null>(null)
  /** The voice call whose conversation is open (its line's entry id). */
  const [callLine, setCallLine] = useState<string | null>(null)
  const [editingGroup, setEditingGroup] = useState(false)
  const [templateDraft, setTemplateDraft] = useState<BotDraft | null>(null)
  const [showSettings, setShowSettings] = useState(false)
  const [detailsTab, setDetailsTab] = useState<DetailsTab>('details')
  const [compactDetails, setCompactDetails] = useState(false)
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
  // The width is measured, not rendered: resizing re-renders only when `wide` flips, or while the
  // window is narrow enough to squeeze the open side panel.
  const width = useRef(800)
  const [wide, setWide] = useState(true)
  const [, setSqueezed] = useState(0)
  const squeezeBelow = useRef(0)
  const [detailsWidth, setDetailsWidth] = usePref(prefs.detailsPanelWidth)
  useReading(store, botId)
  // The chat window remounts this per conversation (`key`), so each one opened counts.
  useScreenViewed('chat')

  useLayoutEffect(() => {
    const el = root.current
    if (!el) return
    const observer = new ResizeObserver(([e]) => {
      const w = e!.contentRect.width
      width.current = w
      setWide(w >= WIDE)
      setSqueezed(w - 380 < squeezeBelow.current ? w : 0)
    })
    observer.observe(el)
    // The composer floats over the transcript; its height becomes the transcript's bottom padding.
    const docked = new ResizeObserver(([e]) => el.style.setProperty('--composer-height', `${Math.ceil(e!.borderBoxSize[0]!.blockSize)}px`))
    if (dock.current) docked.observe(dock.current)
    return () => {
      observer.disconnect()
      docked.disconnect()
    }
  }, [])

  const panelOpen = wide && showSettings
  squeezeBelow.current = panelOpen ? Math.max(360, detailsWidth) : 0
  // The side panel's edge drags its width, leaving the conversation at least 380 wide.
  const panelWidth = (w: number) => Math.min(Math.max(360, w), width.current - 380)
  // A compact Details presentation dismisses once the window is wide enough for the inspector.
  useEffect(() => {
    if (wide) setCompactDetails(false)
  }, [wide])
  const toggleDetails = useCallback(() => {
    if (!wide) setCompactDetails((c) => !c)
    else setShowSettings((s) => !s)
  }, [wide])

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
    setCompactDetails(false)
  }, [])

  const presentRoutine = useCallback((id: string | null) => {
    setRoutineId(id)
    setRoutineRequest((r) => r + 1)
    setDetailsTab('details')
    setShowSettings(true)
    if (!wide) setCompactDetails(true)
  }, [wide])

  // This bot's Agent card is on the Details tab; a group member's is in its settings.
  const presentAgent = useCallback((id: string) => (id === botId ? presentRoutine(null) : setEditingMember(id)), [botId, presentRoutine])
  const member = editingMember ? store.bots.get(editingMember) : undefined

  const details = (
    <DetailsPanel
      key={routineRequest}
      botId={botId}
      routineId={routineId}
      tab={detailsTab}
      setTab={setDetailsTab}
      editGroup={() => setEditingGroup(true)}
      openArtifact={openArtifact}
      share={() => {
        setCompactDetails(false)
        if (bot) setTemplateDraft(draftOf(bot))
      }}
      close={() => {
        setShowSettings(false)
        setCompactDetails(false)
      }}
    />
  )

  return (
    <div className="thread-view" ref={root}>
      <div className="conversation">
        <div className="top-chrome">
          <TopFade />
          <div className="top-chrome-row">
            <Spacer />
            {(!showSettings || !wide) && !compactDetails ? (
              <MacChatButton className="ringed" title="Conversation details" icon="chevron.left.2" direction={[-1, 0]} onClick={toggleDetails} />
            ) : null}
          </div>
          {/* The call bar takes the pill's place; its frosted glass would show the name blurred. */}
          <button className={`title-pill ${calling ? 'hidden-by-call' : ''}`} inert={calling} onClick={toggleDetails} aria-label={`${bot?.name ?? 'Conversation'}, view details`} title="View conversation details">
            <HStack as="span" gap={7}>
              {bot ? isGroup(bot) ? <GroupAvatar members={store.members(bot)} size={22} /> : <BotAvatar bot={bot} size={22} /> : null}
              <Text size="body" weight="medium" tone="text" truncate>{bot?.name ?? ''}</Text>
              {/* Grok Bot's hint on hover: the pill opens the details. */}
              <span className="title-pill-chevron">
                <Icon name="chevron.right" size={10} weight="semibold" color="var(--secondary)" />
              </span>
            </HStack>
            {store.shownConnection.kind !== 'online' || store.mismatch ? <ConnectionSubtitle /> : null}
          </button>
        </div>
        {/* An update card takes the composer's place, so there's nothing to reply with. */}
        <Transcript botId={botId} openTrace={openTrace} onReply={store.mismatch ? undefined : setReplyTo} openRoutine={presentRoutine} openAgent={presentAgent} openBotChat={setBotChat} openArtifact={openArtifact} openCall={setCallLine} />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          {store.mismatch ? (
            <div className="composer-update-needed">
              <UpdateNeededCard />
            </div>
          ) : artifactId ? null : (
            // The artifact over the chat has its own box (two would both take the voice shortcut).
            <Composer
              botId={botId}
              reply={replyTo}
              cancelReply={cancelReply}
              onCall={!calling && bot ? () => setCalling(true) : null}
              onInterrupt={callSpeaking ? () => interruptCall.current?.() : null}
            />
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
        width={panelWidth(detailsWidth)}
        min={360}
        max={width.current - 380}
        onResize={(w) => setDetailsWidth(panelWidth(w))}
      >
        {details}
      </ChatSidePanel>

      <Sheet open={compactDetails} onClose={() => setCompactDetails(false)} width={400} height={600}>
        {details}
      </Sheet>
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
    if (store.mismatch) return 'Needs update'
    return store.hostRoute === 'relay' ? connectedThrough(store.computer.cloud) : store.hostRoute === 'direct' ? 'Connected over Wi-Fi or Tailscale' : store.hostRoute === 'loopback' ? 'Connected locally' : 'Connected'
  })()
  const spoken = tr(description)
  return (
    <HStack as="span" gap={6} className="title-pill-subtitle" title={`${store.hostName} · ${spoken}`} aria-label={`${store.hostName}, ${spoken}`}>
      <IconSlot width={12}>{connecting ? <Spinner size={8} /> : <Icon name={symbol} size={8} weight="medium" />}</IconSlot>
      <Text size="caption" weight="medium" truncate>{store.connectionLabel}</Text>
    </HStack>
  )
}

/** The messages: every loaded message, brought to the newest as it changes while the reader is at the bottom. */
const PAGE = 40

const Transcript = memo(function Transcript({ botId, openTrace, onReply, openRoutine, openAgent, openBotChat, openArtifact, openCall }: { botId: string; openTrace: () => void; onReply?: (e: Entry) => void; openRoutine: (id: string | null) => void; openAgent: (botId: string) => void; openBotChat: (peerId: string) => void; openArtifact: (artifactId: string) => void; openCall: (lineId: string) => void }) {
  const store = useStore()
  const tr = useTr()
  const bot = store.bots.get(botId) ?? null
  const live = !!bot && isWorkingIn(bot, botId) && !store.isOffline
  // Rebuilt only when this chat changes: the store replaces a chat's list on every change.
  const entries = store.allEntries(botId)
  // What voice calls carried shows in each call's conversation, not in the chat.
  const all = useMemo(() => buildChat(hidingCalls(entries)), [entries])
  const steps = useMemo(() => workingSteps(entries), [entries])
  // The newest 40 items first; older pages come in near the top.
  const [firstShown, setFirstShown] = useState<string | null>(null)
  const [loadingEarlier, setLoadingEarlier] = useState(false)
  const found = firstShown ? all.findIndex((i) => i.id === firstShown) : -1
  const start = found >= 0 ? found : Math.max(0, all.length - PAGE)
  const items = all.slice(start)
  const more = start > 0 || store.canLoadOlder(botId)
  // Following the newest message, or reading history: scrolling up releases the bottom,
  // arriving back at the end (or Jump to latest) restores it. Content growth alone never changes it.
  const [following, setFollowing] = useState(true)
  const followingRef = useRef(true)
  followingRef.current = following
  const scroller = useRef<HTMLDivElement>(null)
  const lastScrollTop = useRef(0)
  const keepPlace = useRef<number | null>(null)
  const last = items[items.length - 1]
  const jump = usePresence(!following && items.length > 0, 200)
  const arrivals = useArrivals(items.map((i) => i.id), following)

  const toBottom = (smooth = false) => {
    const el = scroller.current
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' })
  }

  useEffect(() => {
    if (!firstShown && items[0]) setFirstShown(items[0].id)
  }, [firstShown, items])

  const showEarlier = async () => {
    if (loadingEarlier) return
    setLoadingEarlier(true)
    const el = scroller.current
    keepPlace.current = el ? el.scrollHeight - el.scrollTop : null
    let list = all
    let at = start
    if (at === 0) {
      const anchor = list[0]?.id
      await store.loadOlder(botId)
      list = buildChat(hidingCalls(store.allEntries(botId)))
      at = anchor ? Math.max(0, list.findIndex((i) => i.id === anchor)) : 0
    }
    if (at > 0) setFirstShown(list[Math.max(0, at - PAGE)]!.id)
    setLoadingEarlier(false)
  }

  // Earlier messages going in above keep the reader's place; a follower stays at the bottom.
  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    if (keepPlace.current !== null && !followingRef.current) {
      el.scrollTop = el.scrollHeight - keepPlace.current
      keepPlace.current = null
    } else if (followingRef.current) toBottom()
  })

  // A message the user sends brings the chat back to the newest.
  const lastIsUser = last?.kind === 'entry' && last.entry.kind === 'user'
  useEffect(() => {
    if (lastIsUser) setFollowing(true)
  }, [last?.id, lastIsUser])

  // Rows that grow after layout (images, the text size) keep a follower at the bottom.
  useEffect(() => {
    const el = scroller.current
    if (!el) return
    const observer = new ResizeObserver(() => {
      if (followingRef.current) toBottom()
    })
    observer.observe(el.firstElementChild!)
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  return (
    <div className="transcript-frame">
      <div
        className="transcript"
        ref={scroller}
        role="log"
        aria-label="Messages"
        onScroll={(e) => {
          const el = e.currentTarget
          const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < 8
          if (el.scrollTop < lastScrollTop.current - 1 && !atEnd) setFollowing(false)
          else if (atEnd) setFollowing(true)
          lastScrollTop.current = el.scrollTop
          if (more && el.scrollTop < 200 && !loadingEarlier) void showEarlier()
        }}
      >
        <div className="transcript-content">
          {more ? (
            <button className="load-earlier" disabled={loadingEarlier} onClick={() => void showEarlier()}>
              {loadingEarlier ? <Spinner size={14} /> : tr('Load earlier messages')}
            </button>
          ) : null}
          {items.length === 0 && bot ? <div className="transcript-intro">{isGroup(bot) ? <GroupIntroCard group={bot} /> : <IntroCard bot={bot} />}</div> : null}
          {items.map((item) => (
            <div key={item.id} className={arrivals.has(item.id) ? rowIn(item) : undefined}>
              <Row item={item} chat={bot} openTrace={openTrace} onReply={onReply} openRoutine={openRoutine} openAgent={openAgent} openBotChat={openBotChat} openArtifact={openArtifact} openCall={openCall} />
            </div>
          ))}
          {bot ? <WorkingTransition bot={bot} visible={live} steps={steps} /> : null}
          <div className="chat-end" />
        </div>
      </div>
      {jump.mounted ? (
        <div className={`jump-to-latest ${jump.shown ? 'shown' : ''}`} inert={!jump.shown}>
          <MacChatButton
            title="Jump to latest"
            icon="arrow.down"
            direction={[0, 1]}
            onClick={() => {
              setFollowing(true)
              toBottom(true)
            }}
          />
        </div>
      ) : null}
    </div>
  )
})

function Row({ item, chat, openTrace, onReply, openRoutine, openAgent, openBotChat, openArtifact, openCall }: { item: ChatItem; chat: Bot | null; openTrace: () => void; onReply?: (e: Entry) => void; openRoutine: (id: string | null) => void; openAgent: (botId: string) => void; openBotChat: (peerId: string) => void; openArtifact: (artifactId: string) => void; openCall: (lineId: string) => void }) {
  if (item.kind === 'separator') {
    return <TimeSeparator date={item.date} />
  }
  if (item.kind === 'exchanges') return <BotMessageRow exchanges={item.exchanges} open={openBotChat} />
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
    ? `${store.backendName(bot.backend)} · ${tr('Personal workspace')}`
    : tr('{agent} in {path}').replace('{agent}', store.backendName(bot.backend)).replace('{path}', bot.cwd)
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
