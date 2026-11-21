import { useContext, useEffect, useMemo, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import type { Bot } from '@shared/models'
import { isTold } from '@shared/tasks'
import { isLive, readOnlyReason, workStatusWord, type WorkItem } from '@shared/work'
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
import './thread.css'
import './composer.css'
import './chat-controls.css'
import './thread-chat.css'

/**
 * A thread as the window shows it, in the chat's own form: its messages as the chat's bubbles, what its task put in
 * the bot's chat (a card to answer, a file, an artifact) as the chat's own rows, the working line with the turn's
 * steps, and the chat's message box, which writes to the thread. Its title, status and Full conversation are in the
 * window's bar; where it works sits over the box. A routine's run or another bot's request says it is read only.
 */
export function ThreadChat({ bot, item }: { bot: Bot; item: WorkItem }) {
  const tr = useTr()
  const store = useStore()
  const taskId = item.task ?? ''
  const thread = useMemo<ThreadRef>(() => ({ botId: bot.id, itemId: item.id }), [bot.id, item.id])
  useScreenViewed('task', taskId)
  useEffect(() => store.watchTask(taskId), [store, taskId])
  const { entries, told, rest, fetched, failed, error, retry } = useThreadEntries(thread, item)
  const chatOf = () => buildChat(told ? [told, ...rest] : rest)
  const all = useMemo(chatOf, [told, rest])
  const steps = useMemo(() => workingSteps(entries), [entries])
  const live = isLive(item) && !store.isOffline
  // The bot as the working line reads it, with the thread's own activity and time.
  const working = useMemo<Bot>(
    () => ({ ...bot, status: item.status === 'needsYou' ? 'needsInput' : 'working', activity: item.activity, startedAt: item.startedAt ?? undefined }),
    [bot, item.status, item.activity, item.startedAt],
  )
  // The card it waits on is the bot's chat's, maybe further back than the pages loaded: read back to it.
  useEffect(() => {
    if (item.status === 'needsYou') void store.cardOf(bot.id, item.id).catch((e: unknown) => store.setError(e instanceof Error ? e.message : String(e)))
  }, [store, bot.id, item.id, item.status])
  const [artifactId, setArtifactId] = useState<string | null>(null)
  const [showTrace, setShowTrace] = useState(false)
  const [stopping, setStopping] = useState(false)
  const bar = useContext(WindowBarSlots)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)
  const readOnly = readOnlyReason(item)

  const row = (it: ChatItem) => {
    if (it.kind === 'separator') return <TimeSeparator date={it.date} />
    if (it.kind === 'requests') return <div className="chat-row group-start"><RequestRun entries={it.entries} row={(e) => <ChatRow entry={e} groupStart={false} chat={bot} />} /></div>
    const e = it.entry
    const start = it.kind === 'entry' && it.groupStart
    const cls = start ? 'chat-row group-start' : 'chat-row'
    // What the task put in the bot's chat is that chat's entry, answered and opened here as there.
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

  return (
    <div className="thread-view thread-chat" data-work-thread={item.id}>
      <div className="conversation" ref={root}>
        <div className="top-chrome">
          <div className="top-fade" />
          {bar.title ? createPortal(<>
            <div className="title-pill" title={item.title}>
              <HStack as="span" gap={7}>
                <BotAvatar bot={bot} size={22} animated={false} />
                <Text size="body" weight="medium" tone="text" truncate>{item.title}</Text>
              </HStack>
            </div>
            <HStack as="span" gap={6} className="window-bar-status">
              <WorkDot mark={WORK_MARK[item.status]} />
              <Text size="body" tone="secondary">{tr(workStatusWord(item.status))}</Text>
              {item.startedAt !== null ? <Timer since={item.startedAt} until={item.endedAt ?? undefined} /> : null}
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
              {/* The bot's own folder goes without saying, as in the list. */}
              <div className="thread-options">{bot.managedWorkspace && !item.worktree && item.folder === bot.cwd ? null : <WhereCopy item={item} />}</div>
              {item.talk ? (
                <Composer
                  botId={bot.id}
                  thread={{
                    // ponytail: a draft left for an item that is gone stays saved until then; drop it with the item if they pile up.
                    draftKey: `${bot.id}/thread/${item.id}`,
                    placeholder: tr('Message this thread'),
                    working: isLive(item),
                    send: (text) => store.sendToThread(thread, taskId, text),
                    stop: () => setStopping(true),
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
        <TracePage entries={entries} working={isLive(item)} />
      </Sheet>
      <StopWorkDialog botId={bot.id} itemId={stopping ? item.id : null} onClose={() => setStopping(false)} />
    </div>
  )
}
