import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react'
import { draftOf, isGroup, isWorkingIn, type Bot, type BotDraft } from '@shared/models'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { Sheet } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { UpdateNeededCard } from '../UpdateNeededCard'
import { buildChat, rowIn, useArrivals, type ChatItem } from './chat-items'
import { BotConversationView, BotMessageRow } from './BotConversation'
import { ChatRow, TimeSeparator, WorkingIndicator } from './ChatRows'
import { Composer } from './Composer'
import { ChatSidePanel } from './ChatSidePanel'
import { DetailsPanel } from './DetailsPanel'
import { RepliesView } from './RepliesView'
import { TraceView } from './TraceView'
import { useReading } from './reading'
import { GroupEditorView } from '../bots/GroupEditorView'
import { BotTemplateView } from '../bots/BotTemplateView'
import { CallView } from '../call/CallView'
import { MacChatButton } from './MacChatButton'
import './thread.css'
import './composer.css'
import './chat-controls.css'

const WIDE = 680

/**
 * One endless conversation with a bot or a group chat. Only deliberate messages show here; tool
 * calls and thinking live in the "Full conversation" sheet. Any message can start a thread,
 * which opens beside the chat.
 */
export function ThreadView({ botId }: { botId: string }) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const [showTrace, setShowTrace] = useState(false)
  const [openThread, setOpenThread] = useState<string | null>(null)
  const [botChat, setBotChat] = useState<string | null>(null)
  const [editingGroup, setEditingGroup] = useState(false)
  const [templateDraft, setTemplateDraft] = useState<BotDraft | null>(null)
  const [showSettings, setShowSettings] = useState(false)
  const [editingDetails, setEditingDetails] = useState(false)
  const [compactDetails, setCompactDetails] = useState(false)
  const [calling, setCalling] = useState(false)
  const [callSpeaking, setCallSpeaking] = useState(false)
  const interruptCall = useRef<(() => void) | null>(null)
  const [routineId, setRoutineId] = useState<string | null>(null)
  const [routineRequest, setRoutineRequest] = useState(0)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(800)
  useReading(store, botId, null)

  useLayoutEffect(() => {
    const el = root.current
    if (!el) return
    const observer = new ResizeObserver(([e]) => setWidth(e!.contentRect.width))
    observer.observe(el)
    // The composer floats over the transcript; its height becomes the transcript's bottom padding.
    const docked = new ResizeObserver(([e]) => el.style.setProperty('--composer-height', `${Math.ceil(e!.borderBoxSize[0]!.blockSize)}px`))
    if (dock.current) docked.observe(dock.current)
    return () => {
      observer.disconnect()
      docked.disconnect()
    }
  }, [])

  const wide = width >= WIDE
  // A compact Details presentation dismisses once the window is wide enough for the inspector.
  useEffect(() => {
    if (wide) setCompactDetails(false)
  }, [wide])
  const toggleDetails = useCallback(() => {
    if (!wide) {
      setOpenThread(null)
      setCompactDetails((c) => !c)
    } else if (openThread) {
      // The thread occupies the inspector slot: << shows details instead of toggling behind it.
      setOpenThread(null)
      setShowSettings(true)
    } else setShowSettings((s) => !s)
  }, [wide, openThread])

  useEffect(
    () =>
      window.codync.app.onCommand((c) => {
        if (c.kind === 'toggleDetails') toggleDetails()
      }),
    [toggleDetails],
  )

  const openTrace = useCallback(() => setShowTrace(true), [])
  const openThreadOn = useCallback((root: { id: string }) => setOpenThread(root.id), [])
  const closeThread = useCallback(() => setOpenThread(null), [])

  const presentRoutine = (id: string | null) => {
    setRoutineId(id)
    setRoutineRequest((r) => r + 1)
    setEditingDetails(false)
    setOpenThread(null)
    setShowSettings(true)
    if (!wide) setCompactDetails(true)
  }

  const details = (
    <DetailsPanel
      key={routineRequest}
      botId={botId}
      routineId={routineId}
      editing={editingDetails}
      setEditing={setEditingDetails}
      editGroup={() => setEditingGroup(true)}
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
            <span style={{ flex: 1 }} />
            {(!showSettings || openThread || !wide) && !compactDetails ? (
              <MacChatButton title="Conversation details" icon="chevron.left.2" direction={[-1, 0]} onClick={toggleDetails} />
            ) : null}
          </div>
          {/* The call bar takes the pill's place; its frosted glass would show the name blurred. */}
          <button className={`title-pill ${calling ? 'hidden-by-call' : ''}`} inert={calling} onClick={toggleDetails} aria-label="View conversation details" title="View conversation details">
            <span style={{ display: 'flex', alignItems: 'center', gap: 7 }}>
              {bot ? isGroup(bot) ? <GroupAvatar members={store.members(bot)} size={22} /> : <BotAvatar bot={bot} size={22} /> : null}
              <span style={{ ...font(13, 'medium'), color: 'var(--text)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{bot?.name ?? ''}</span>
            </span>
            {store.shownConnection.kind !== 'online' || store.mismatch ? <ConnectionSubtitle /> : null}
          </button>
        </div>
        <Transcript botId={botId} openTrace={openTrace} openThread={openThreadOn} openRoutine={presentRoutine} openBotChat={setBotChat} />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          {store.mismatch ? (
            <div style={{ padding: '0 16px 8px' }}>
              <UpdateNeededCard />
            </div>
          ) : (
            <Composer
              botId={botId}
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
      </div>
      <ChatSidePanel open={wide && (!!openThread || showSettings)} width={openThread ? Math.min(420, width * 0.45) : 292}>
        {openThread ? <RepliesView key={openThread} botId={botId} rootId={openThread} close={closeThread} /> : details}
      </ChatSidePanel>

      <Sheet open={compactDetails} onClose={() => setCompactDetails(false)} width={340} height={600}>
        {details}
      </Sheet>
      <Sheet open={openThread !== null && !wide} onClose={closeThread} width={440} height={600}>
        {openThread ? <RepliesView botId={botId} rootId={openThread} close={closeThread} /> : null}
      </Sheet>
      <Sheet open={botChat !== null} onClose={() => setBotChat(null)} width={620} height={560}>
        {botChat ? <BotConversationView botId={botId} peerId={botChat} /> : null}
      </Sheet>
      <Sheet open={showTrace} onClose={() => setShowTrace(false)} width={620} height={560}>
        <TraceView botId={botId} />
      </Sheet>
      <Sheet open={templateDraft !== null} onClose={() => setTemplateDraft(null)}>
        {templateDraft ? <BotTemplateView draft={templateDraft} /> : null}
      </Sheet>
      <Sheet open={editingGroup} onClose={() => setEditingGroup(false)} width={420} height={560}>
        <GroupEditorView group={bot} />
      </Sheet>
    </div>
  )
}

/** The top edge of the chat: messages fade out gradually as they scroll under the title pill. */
function TopFade() {
  return <div className="top-fade" />
}

function ConnectionSubtitle() {
  const store = useStore()
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
    return store.hostRoute === 'relay' ? 'Connected through Cloudflare' : store.hostRoute === 'direct' ? 'Connected over Wi-Fi or Tailscale' : store.hostRoute === 'loopback' ? 'Connected locally' : 'Connected'
  })()
  return (
    <span style={{ display: 'flex', alignItems: 'center', gap: 6, ...font(10, 'medium'), color: 'var(--secondary)' }} title={`${store.hostName} · ${description}`} aria-label={`${store.hostName}, ${description}`}>
      <span style={{ width: 12, display: 'flex', justifyContent: 'center' }}>{connecting ? <Spinner size={8} /> : <Icon name={symbol} size={8} weight="medium" />}</span>
      <span style={{ whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{store.connectionLabel}</span>
    </span>
  )
}

/** The messages: every loaded message, brought to the newest as it changes while the reader is at the bottom. */
const PAGE = 40

function Transcript({ botId, openTrace, openThread, openRoutine, openBotChat }: { botId: string; openTrace: () => void; openThread: (e: { id: string }) => void; openRoutine: (id: string | null) => void; openBotChat: (peerId: string) => void }) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const thread = store.chat(botId)
  const live = !!bot && isWorkingIn(bot, botId, null) && !store.isOffline
  const all = buildChat(thread, true)
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
  const arrivals = useArrivals([...items.map((i) => i.id), ...(live ? ['working'] : [])], following)

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
      list = buildChat(store.chat(botId), true)
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
              {loadingEarlier ? <Spinner size={14} /> : 'Load earlier messages'}
            </button>
          ) : null}
          {items.length === 0 && bot ? <div style={{ paddingTop: 40 }}>{isGroup(bot) ? <GroupIntroCard group={bot} /> : <IntroCard bot={bot} />}</div> : null}
          {items.map((item) => (
            <div key={item.id} className={arrivals.has(item.id) ? rowIn(item) : undefined}>
              <Row item={item} chat={bot} openTrace={openTrace} openThread={openThread} openRoutine={openRoutine} openBotChat={openBotChat} />
            </div>
          ))}
          {bot && live ? (
            <div className={arrivals.has('working') ? 'row-in' : undefined} style={{ paddingTop: 6 }}>
              <WorkingIndicator bot={bot} thinking={store.currentThinking(botId, null)} />
            </div>
          ) : null}
          <div style={{ height: 8 }} />
        </div>
      </div>
      {!following && items.length ? (
        <div className="jump-to-latest" style={{ bottom: 'calc(var(--composer-height, 70px) + 10px)' }}>
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
}

function Row({ item, chat, openTrace, openThread, openRoutine, openBotChat }: { item: ChatItem; chat: Bot | null; openTrace: () => void; openThread: (e: { id: string }) => void; openRoutine: (id: string | null) => void; openBotChat: (peerId: string) => void }) {
  if (item.kind === 'separator') {
    return <TimeSeparator date={item.date} />
  }
  if (item.kind === 'exchanges') return <BotMessageRow exchanges={item.exchanges} open={openBotChat} />
  const e = item.entry
  if (e.kind === 'notice' && e.data.routineId) {
    return (
      <button onClick={() => openRoutine(e.data.routineId!)} style={{ display: 'flex', alignItems: 'center', gap: 6, ...font('footnote'), color: 'var(--secondary)', padding: '10px 0', margin: '0 auto' }}>
        <Icon name="clock.arrow.circlepath" size={10} />
        {e.data.text ?? 'Routine'}
      </button>
    )
  }
  return <ChatRow entry={e} groupStart={item.groupStart} chat={chat} openTrace={openTrace} openThread={openThread} />
}

/** The start of an empty group chat: who's in it and how the room works. */
function GroupIntroCard({ group }: { group: Bot }) {
  const store = useStore()
  const members = store.members(group)
  const names = members.map((m) => m.name)
  return (
    <div className="intro-card">
      <GroupAvatar members={members} size={72} />
      <div style={{ ...font('title2', 'semibold'), color: 'var(--text)' }}>{group.name}</div>
      {group.name !== names.join(', ') ? <div style={{ ...font('subheadline'), color: 'var(--secondary)' }}>{names.join(' · ')}</div> : null}
      {group.description ? <div style={{ ...font('subheadline'), color: 'var(--secondary)' }}>{group.description}</div> : null}
      <div style={{ ...font('callout'), color: 'var(--secondary)', paddingTop: 4, lineHeight: 1.5, maxWidth: 360, textWrap: 'balance' }}>Everyone answers in turn. @mention a bot to ask just that one.</div>
    </div>
  )
}

function IntroCard({ bot }: { bot: Bot }) {
  const store = useStore()
  return (
    <div className="intro-card">
      <BotAvatar bot={bot} size={72} />
      <div style={{ ...font('title2', 'semibold'), color: 'var(--text)' }}>{bot.name}</div>
      <div style={{ ...font('callout'), color: 'var(--secondary)', overflowWrap: 'anywhere' }}>
        {bot.managedWorkspace ? `${store.backendName(bot.backend)} · Personal workspace` : `${store.backendName(bot.backend)} in ${bot.cwd}`}
      </div>
      {bot.description ? <div style={{ ...font('subheadline'), color: 'var(--secondary)' }}>{bot.description}</div> : null}
      <div style={{ ...font('callout'), color: 'var(--secondary)', paddingTop: 4, lineHeight: 1.5 }}>Tell it what you need. You'll get a notification when it's done or needs you.</div>
    </div>
  )
}
