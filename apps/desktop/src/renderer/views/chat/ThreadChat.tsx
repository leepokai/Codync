import { useContext, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { draftOf, type Bot, type BotDraft } from '@shared/models'
import { isTold } from '@shared/threads'
import { agentOf, isLive, readOnlyReason, workStatusWord, type WorkTask } from '@shared/work'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar } from '../../components/Avatar'
import { IconButton, Spinner } from '../../components/Controls'
import { Sheet } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { prefs, usePref } from '../../lib/prefs'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import type { ThreadRef } from '../../store/bot-store'
import { useStore } from '../../store/context'
import { BotTemplateView } from '../bots/BotTemplateView'
import { ArtifactView } from '../library/ArtifactView'
import { WindowBarSlots } from '../WindowBar'
import { closeView, PANEL_VIEWS, showView, type PanelPane as Pane } from '@shared/panel-layout'
import { buildChat, type ChatItem } from './chat-items'
import { ChatSidePanel, useSidePanelWidth } from './ChatSidePanel'
import { CONVERSATION_MIN } from './side-panel-width'
import { SideChat } from './ChatView'
import { DetailsPanel } from './DetailsPanel'
import { ChatLog, useDockHeight } from './ChatLog'
import { BackgroundCapsule } from './BackgroundCapsule'
import { AgentBubble, ChatRow, TimeSeparator } from './ChatRows'
import { Composer } from './Composer'
import { PanelBarButtons } from './PanelBarButtons'
import { closeTerminal } from './TerminalTab'
import { MultiTab, multiTabViews, PanelPane } from './PanelPane'
import { RequestRun } from './PermissionCard'
import { workingSteps } from './trace-turns'
import { StopWorkDialog, WORK_MARK, WorkDot } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import { WorkingTransition } from './WorkingTransition'
import { WhereCopy } from './WorkWhere'
import { Mine, ThreadReadFailed, Told, useThreadEntries } from './WorkThreadTranscript'
import './chat.css'
import './composer.css'
import './chat-controls.css'
import './thread-chat.css'
import './details-panel.css'

/** What the panel of a thread without a bot can show: a web page and a terminal in its folder (a bot's thread shows its
 * bot's whole panel). */
const THREAD_VIEWS = PANEL_VIEWS.filter((v) => v.id === 'browser' || v.id === 'terminal')
const NEW_PANE: Pane = { tabs: [], shown: null }

/**
 * A thread as the window shows it, in the chat's own form: its messages as the chat's bubbles, what its task put in
 * the bot's chat (a card to answer, a file, an artifact) as the chat's own rows, the working line with the turn's
 * steps, what its agent runs in the background in a capsule over the box, and the chat's message box, which writes to the thread, with the agent it runs on and where it works in the box's
 * row. Its title, status and the panel's toggle are in the window's bar (as a tab of a bot's panel, with no bar, the
 * status sits on top and the tab names it); the panel beside it is its bot's own (Details, Computer, Library, Memory, Settings, Browser, Terminal in the thread's
 * folder, Side chat with the bot, its other threads), a thread without a bot's a Browser and a Terminal; what the panel shows in the chat opens the bot's chat; where it works sits over the box. A routine's run or another bot's request says it is read only.
 */
export function ThreadChat({ bot, task, openChat }: { bot: Bot; task: WorkTask; openChat: () => void }) {
  const tr = useTr()
  const store = useStore()
  const threadId = task.thread ?? ''
  const thread = useMemo<ThreadRef>(() => ({ botId: bot.id, taskId: task.id }), [bot.id, task.id])
  useScreenViewed('thread', threadId)
  useEffect(() => store.watchThread(threadId), [store, threadId])
  const { entries, told, rest, fetched, failed, error, retry } = useThreadEntries(thread, task)
  const chatOf = () => buildChat(told ? [told, ...rest] : rest)
  const all = useMemo(chatOf, [told, rest])
  const steps = useMemo(() => workingSteps(entries), [entries])
  const live = isLive(task) && !store.isOffline
  // The bot as the working line reads it, with the thread's own activity and time.
  const working = useMemo<Bot>(
    () => ({ ...bot, status: task.status === 'needsYou' ? 'needsInput' : 'working', activity: task.activity, startedAt: task.startedAt ?? undefined }),
    [bot, task.status, task.activity, task.startedAt],
  )
  // The card it waits on is the bot's chat's, maybe further back than the pages loaded: read back to it.
  useEffect(() => {
    if (task.status === 'needsYou') void store.cardOf(bot.id, task.id).catch((e: unknown) => store.setError(e instanceof Error ? e.message : String(e)))
  }, [store, bot.id, task.id, task.status])
  const [artifactId, setArtifactId] = useState<string | null>(null)
  const [panelOpen, setPanelOpen] = useState(false)
  const [panelFilled, setPanelFilled] = useState(false)
  const [pane, setPane] = useState<Pane>(NEW_PANE)
  // Its panel goes when the thread leaves the window, and its terminals with it.
  const tabs = useRef(pane.tabs)
  useEffect(() => { tabs.current = pane.tabs }, [pane.tabs])
  useEffect(() => () => { for (const tab of tabs.current) closeTerminal(store.client, tab) }, [store])
  const [templateDraft, setTemplateDraft] = useState<BotDraft | null>(null)
  const [panelWidth, setPanelWidth] = usePref(prefs.detailsPanelWidth)
  const view = useRef<HTMLDivElement>(null)
  const panel = useSidePanelWidth(view, CONVERSATION_MIN, panelWidth)
  const [stopping, setStopping] = useState(false)
  const bar = useContext(WindowBarSlots)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)
  const readOnly = readOnlyReason(task)
  // An artifact opens over the conversation, so the panel stops covering it.
  const openArtifact = (id: string) => {
    setArtifactId(id)
    setPanelFilled(false)
    if (!panel.beside) setPanelOpen(false)
  }

  const row = (it: ChatItem) => {
    if (it.kind === 'separator') return <TimeSeparator date={it.date} />
    if (it.kind === 'requests') return <div className="chat-row group-start"><RequestRun entries={it.entries} row={(e) => <ChatRow entry={e} groupStart={false} chat={bot} />} /></div>
    const e = it.entry
    const start = it.kind === 'entry' && it.groupStart
    const cls = start ? 'chat-row group-start' : 'chat-row'
    // What the thread put in the bot's chat is that chat's entry, answered and opened here as there.
    if (e.botId === bot.id || it.kind === 'exchanges') return <ChatRow entry={e} groupStart={start} chat={bot} openArtifact={setArtifactId} />
    if (e.id === told?.id) return <div className="chat-row group-start"><Told entry={e} /></div>
    // What the host told it later (a new brief) reads as a quote, not as something the user typed.
    if (isTold(e)) return <div className="chat-row group-start"><Text as="div" size="footnote" tone="secondary" className="selectable work-thread-quote">{e.data.text ?? ''}</Text></div>
    if (e.kind === 'user') {
      return (
        <VStack align="flex-end" gap={4} className={cls}>
          <div className="message user"><div className="message-body"><Mine entry={e} thread={thread} /></div></div>
        </VStack>
      )
    }
    if (e.kind === 'agent') return <VStack align="flex-start" gap={4} className={cls}><AgentBubble entry={e} plain openArtifact={setArtifactId} /></VStack>
    return <ChatRow entry={e} groupStart={start} chat={bot} />
  }

  const ownFolder = bot.managedWorkspace && !task.worktree && task.folder === bot.cwd
  const agent = agentOf(task, bot) ?? bot.backend
  const status = (
    <HStack as="span" gap={6} className="window-bar-status">
      <WorkDot mark={WORK_MARK[task.status]} />
      <Text size="body" tone="secondary">{tr(workStatusWord(task.status))}</Text>
      {task.startedAt !== null ? <Timer since={task.startedAt} until={task.endedAt ?? undefined} /> : null}
    </HStack>
  )
  return (
    <div className="chat-view thread-chat" data-work-thread={task.id} ref={view}>
      {/* Covered by the panel, it leaves the keyboard's reach too. */}
      <div className="conversation" ref={root} inert={panelOpen && (panelFilled || !panel.beside)}>
        <div className="top-chrome">
          <div className="top-fade" />
          {bar.title ? createPortal(<>
            <div className="title-pill" title={task.title}>
              <HStack as="span" gap={7} className="thread-title">
                {bot.plain ? <AgentIcon registry={store.agentRegistry(agent)} size={18} /> : <BotAvatar bot={bot} size={22} animated={false} />}
                <Text size="body" weight="medium" tone="text" truncate>{task.title}</Text>
              </HStack>
            </div>
            {status}
          </>, bar.title) : <div className="thread-chat-status">{status}</div>}
          {/* The bot's chat's buttons, in its order; a thread without a bot has no template and one pane, so only Maximize. */}
          {bar.end ? createPortal(<PanelBarButtons
            open={panelOpen}
            filled={panelFilled}
            setFilled={panel.beside ? setPanelFilled : undefined}
            share={bot.plain ? undefined : () => setTemplateDraft(draftOf(bot))}
            toggle={() => setPanelOpen((o) => !o)}
          />, bar.end) : null}
        </div>
        <ChatLog
          items={all}
          fresh={chatOf}
          canLoadOlder={false}
          loadOlder={async () => {}}
          intro={failed ? null : fetched ? <Text as="div" size="footnote" tone="tertiary" className="work-thread-note">{tr('Nothing yet.')}</Text> : <div className="work-thread-note" aria-label={tr('Loading')}><Spinner /></div>}
          row={row}
          working={<>
            {failed ? <ThreadReadFailed error={error} retry={retry} /> : null}
            <WorkingTransition bot={working} visible={live} steps={steps} />
          </>}
        />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          {artifactId ? null : (
            <>
              <BackgroundCapsule task={task} />
              {task.talk ? (
                <Composer
                  botId={bot.id}
                  thread={{
                    // ponytail: a draft left for a task that is gone stays saved until then; drop it with the task if they pile up.
                    draftKey: `${bot.id}/thread/${task.id}`,
                    placeholder: tr('Message this thread'),
                    working: isLive(task),
                    files: task.talkFiles,
                    send: (text, files) => store.sendToThread(thread, threadId, text, { files }),
                    stop: () => setStopping(true),
                    // The agent it runs on, then where it works; the bot's own folder goes without saying, as in the list.
                    controls: <>
                      <span className="thread-option static" title={tr('Agent')}>
                        <AgentIcon registry={store.agentRegistry(agent)} size={14} />
                        <Text size="caption" truncate>{store.backendName(agent)}</Text>
                      </span>
                      {ownFolder ? null : <WhereCopy task={task} />}
                    </>,
                  }}
                />
              ) : readOnly ? (
                // It can't be written to, but a run under way can still be stopped.
                <HStack gap={8} justify="center" className="work-thread-read-only">
                  <Text size="caption" tone="tertiary">{tr(readOnly)}</Text>
                  {isLive(task) ? <IconButton title="Stop" icon="stop.fill" size={24} disabled={store.isOffline} onClick={() => setStopping(true)} /> : null}
                </HStack>
              ) : null}
            </>
          )}
        </div>
        {artifactId ? <ArtifactView botId={bot.id} artifactId={artifactId} close={() => setArtifactId(null)} /> : null}
      </div>
      <ChatSidePanel open={panelOpen} filled={!panel.beside || panelFilled} width={panel.width} max={panel.max} onResize={(w) => setPanelWidth(panel.fit(w))}>
        {bot.plain ? <div className="details-panel"><div className="details-page"><div className="details-panes">
          <PanelPane
            pane={pane}
            views={[...THREAD_VIEWS, ...multiTabViews(pane.tabs, THREAD_VIEWS, tr)]}
            first
            flush={pane.shown !== null}
            show={(id) => setPane((p) => showView([p], id)[0] ?? p)}
            close={(id) => {
              closeTerminal(store.client, id)
              setPane((p) => closeView([p], 0, id)[0] ?? p)
            }}
            choose={() => setPane((p) => ({ ...p, shown: null }))}
          >
            {pane.shown ? <MultiTab bot={bot} thread={threadId} tab={pane.shown} /> : null}
          </PanelPane>
        </div></div></div> : (
          <DetailsPanel
            botId={bot.id}
            routineId={null}
            docked={bar.end !== null}
            thread={task}
            sideChat={<SideChat botId={bot.id} openArtifact={openArtifact} />}
            editGroup={() => {}}
            openArtifact={openArtifact}
            openCard={openChat}
            share={() => setTemplateDraft(draftOf(bot))}
            close={() => setPanelOpen(false)}
          />
        )}
      </ChatSidePanel>
      <Sheet open={templateDraft !== null} onClose={() => setTemplateDraft(null)}>
        {templateDraft ? <BotTemplateView draft={templateDraft} /> : null}
      </Sheet>
      <StopWorkDialog botId={bot.id} taskId={stopping ? task.id : null} onClose={() => setStopping(false)} />
    </div>
  )
}
