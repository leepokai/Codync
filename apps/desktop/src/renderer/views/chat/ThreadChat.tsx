import { useContext, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Bot } from '@shared/models'
import { isTold } from '@shared/threads'
import { isLive, readOnlyReason, workStatusWord, type WorkTask } from '@shared/work'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar } from '../../components/Avatar'
import { IconButton, Spinner } from '../../components/Controls'
import { Sheet } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import type { ThreadRef } from '../../store/bot-store'
import { useStore } from '../../store/context'
import { ArtifactView } from '../library/ArtifactView'
import { WindowBarSlots } from '../WindowBar'
import { buildChat, type ChatItem } from './chat-items'
import { ChatLog, useDockHeight } from './ChatLog'
import { AgentBubble, ChatRow, TimeSeparator } from './ChatRows'
import { Composer } from './Composer'
import { RequestRun } from './PermissionCard'
import { TracePage } from './TraceView'
import { workingSteps } from './trace-turns'
import { StopWorkDialog, WORK_MARK, WorkDot } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import { WorkingTransition } from './WorkingTransition'
import { WhereCopy } from './WorkThread'
import { Mine, ThreadReadFailed, Told, useThreadEntries } from './WorkThreadTranscript'
import './chat.css'
import './composer.css'
import './chat-controls.css'
import './thread-chat.css'

/**
 * A thread as the window shows it, in the chat's own form: its messages as the chat's bubbles, what its task put in
 * the bot's chat (a card to answer, a file, an artifact) as the chat's own rows, the working line with the turn's
 * steps, and the chat's message box, which writes to the thread. Its title, status and Full conversation are in the
 * window's bar; where it works sits over the box. A routine's run or another bot's request says it is read only.
 */
export function ThreadChat({ bot, task }: { bot: Bot; task: WorkTask }) {
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
  const [showTrace, setShowTrace] = useState(false)
  const [stopping, setStopping] = useState(false)
  const bar = useContext(WindowBarSlots)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)
  const readOnly = readOnlyReason(task)

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
  return (
    <div className="chat-view thread-chat" data-work-thread={task.id}>
      <div className="conversation" ref={root}>
        <div className="top-chrome">
          <div className="top-fade" />
          {bar.title ? createPortal(<>
            <div className="title-pill" title={task.title}>
              <HStack as="span" gap={7} className="thread-title">
                {bot.plain ? <AgentIcon registry={store.agentRegistry(task.agent ?? bot.backend)} size={18} /> : <BotAvatar bot={bot} size={22} animated={false} />}
                <Text size="body" weight="medium" tone="text" truncate>{task.title}</Text>
              </HStack>
            </div>
            <HStack as="span" gap={6} className="window-bar-status">
              <WorkDot mark={WORK_MARK[task.status]} />
              <Text size="body" tone="secondary">{tr(workStatusWord(task.status))}</Text>
              {task.startedAt !== null ? <Timer since={task.startedAt} until={task.endedAt ?? undefined} /> : null}
            </HStack>
          </>, bar.title) : null}
          {bar.end ? createPortal(<IconButton title="Full conversation" icon="list.bullet" size={28} onClick={() => setShowTrace(true)} />, bar.end) : null}
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
              {task.talk ? (
                <Composer
                  botId={bot.id}
                  thread={{
                    // ponytail: a draft left for a task that is gone stays saved until then; drop it with the task if they pile up.
                    draftKey: `${bot.id}/thread/${task.id}`,
                    placeholder: tr('Message this thread'),
                    working: isLive(task),
                    send: (text) => store.sendToThread(thread, threadId, text),
                    stop: () => setStopping(true),
                    // Where it works, in the box's row; the bot's own folder goes without saying, as in the list.
                    controls: ownFolder ? null : <WhereCopy task={task} />,
                  }}
                />
              ) : readOnly ? (
                <Text as="div" size="caption" tone="tertiary" className="work-thread-read-only">{tr(readOnly)}</Text>
              ) : null}
            </>
          )}
        </div>
        {artifactId ? <ArtifactView botId={bot.id} artifactId={artifactId} close={() => setArtifactId(null)} /> : null}
      </div>
      <Sheet open={showTrace} onClose={() => setShowTrace(false)} width={620} height={560}>
        <TracePage entries={entries} working={isLive(task)} />
      </Sheet>
      <StopWorkDialog botId={bot.id} taskId={stopping ? task.id : null} onClose={() => setStopping(false)} />
    </div>
  )
}
