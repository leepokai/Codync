import { useEffect, useRef, useState, useSyncExternalStore } from 'react'
import { isLive, readOnlyReason, whereLine, wherePath, workStatusWord, type WorkTask } from '@shared/work'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useScreenViewed } from '../../store/analytics'
import type { ThreadRef } from '../../store/bot-store'
import { useStore } from '../../store/context'
import { ComposerField } from './ComposerField'
import { useFollowEnd } from './follow-end'
import { StopWorkDialog, WORK_MARK, WorkDot } from './WorkDialogs'
import { Timer } from './WorkingIndicator'
import { WorkThreadTranscript } from './WorkThreadTranscript'
import { WorkWhere } from './WorkWhere'
import './composer.css'
import './work.css'

/** How long `Path copied` stands in for the where line. */
const COPIED_MS = 1600

/**
 * The thread of one task, in the inspector in place of Details: its title, status, time and where it works, the
 * thread's transcript, and a box to write to it (the bot's own tasks; a routine's run or another bot's request says it
 * is read only). `close` goes back to Details; Escape does too while the panel has the keyboard. `showInChat` brings an
 * entry of the bot's chat into view beside it.
 */
export function WorkThread({ botId, task, openArtifact, showInChat, close }: { botId: string; task: WorkTask; openArtifact: (artifactId: string) => void; showInChat: (entryId: string) => void; close: () => void }) {
  const tr = useTr()
  const store = useStore()
  const threadId = task.thread ?? ''
  const thread: ThreadRef = { botId, taskId: task.id }
  useScreenViewed('thread', threadId)
  useEffect(() => store.watchThread(threadId), [store, threadId])
  const [stopping, setStopping] = useState(false)
  const follow = useFollowEnd()
  const back = useRef<HTMLButtonElement>(null)
  const talks = task.talk
  const readOnly = readOnlyReason(task)
  // Without a box to type in, the keyboard starts on Back.
  useEffect(() => {
    if (!talks) back.current?.focus({ preventScroll: true })
  }, [talks])
  return (
    <div
      className="work-thread"
      data-work-thread={task.id}
      onKeyDown={(e) => {
        if (e.key !== 'Escape' || e.defaultPrevented || e.nativeEvent.isComposing) return
        e.stopPropagation()
        close()
      }}
    >
      <HStack gap={8} className="work-thread-header">
        <IconButton buttonRef={back} title="Back to details" icon="chevron.left" onClick={close} />
        <VStack gap={2} grow shrink>
          <Text as="div" size="body" weight="semibold" tone="text" truncate title={task.title}>{task.title}</Text>
          <HStack gap={8} className="work-thread-meta">
            <HStack as="span" gap={5} className="work-thread-status">
              <WorkDot mark={WORK_MARK[task.status]} />
              <Text size="caption" tone="secondary">{tr(workStatusWord(task.status))}</Text>
              {task.startedAt !== null ? <Timer since={task.startedAt} until={task.endedAt ?? undefined} /> : null}
            </HStack>
            <WhereCopy task={task} />
          </HStack>
        </VStack>
        {isLive(task) ? <IconButton title="Stop" icon="stop.fill" disabled={store.isOffline} onClick={() => setStopping(true)} /> : null}
      </HStack>
      <div ref={follow.ref} className="work-thread-scroll" role="log" aria-label={tr('Messages')} onScroll={follow.onScroll}>
        <WorkThreadTranscript thread={thread} task={task} openArtifact={openArtifact} showInChat={showInChat} />
      </div>
      {talks ? (
        <ThreadComposer thread={thread} threadId={threadId} onSent={follow.toEnd} />
      ) : readOnly ? (
        <Text as="div" size="caption" tone="tertiary" className="work-thread-read-only">{tr(readOnly)}</Text>
      ) : null}
      <StopWorkDialog botId={botId} taskId={stopping ? task.id : null} onClose={() => setStopping(false)} />
    </div>
  )
}

/** Where the task works, as a button that copies the full path and says so for a moment. Nothing for a task whose
 * folder the host doesn't name. */
function WhereCopy({ task }: { task: WorkTask }) {
  const tr = useTr()
  const store = useStore()
  const where = whereLine(task, store.hello?.home ?? '')
  const path = wherePath(task)
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    if (!copied) return
    const timer = setTimeout(() => setCopied(false), COPIED_MS)
    return () => clearTimeout(timer)
  }, [copied])
  if (!where || !path) return null
  return (
    <button
      className="work-where-copy"
      title={tr('Copy path')}
      aria-label={`${tr('Copy path')}: ${path}`}
      onClick={() => {
        window.codync.app.copy(path)
        setCopied(true)
      }}
    >
      {copied ? (
        <HStack as="span" gap={4} className="fade-in" role="status">
          <Icon name="checkmark" size={10} color="var(--secondary)" />
          <Text size="caption" tone="secondary">{tr('Path copied')}</Text>
        </HStack>
      ) : (
        <WorkWhere where={where} />
      )}
    </button>
  )
}

/** The chat's message box, text only: Return sends to the thread, where the message waits while the thread's turn runs. */
function ThreadComposer({ thread, threadId, onSent }: { thread: ThreadRef; threadId: string; onSent: () => void }) {
  const tr = useTr()
  const store = useStore()
  const drafts = store.composerDrafts
  // Under the bot's key, so the draft goes with the bot (`removeBot`).
  // ponytail: a draft left for a task that is gone stays saved until then; drop it with the task if they pile up.
  const key = `${thread.botId}/thread/${thread.taskId}`
  const draft = useSyncExternalStore(drafts.subscribe, () => drafts.get(key))
  const field = useRef<HTMLTextAreaElement>(null)
  const label = tr('Message this thread')
  const canSend = !!draft.trim() && !store.isOffline
  useEffect(() => field.current?.focus({ preventScroll: true }), [])
  const submit = () => {
    if (!canSend) return
    onSent()
    store.sendToThread(thread, threadId, draft)
    drafts.set(key, '')
    field.current?.focus()
  }
  return (
    <div className="composer work-thread-composer">
      <div className="composer-surface">
        <div className="composer-box">
          <ComposerField field={field} value={draft} placeholder={label} label={label} onChange={(text) => drafts.set(key, text)} onSend={submit} />
          <button className="press composer-send" disabled={!canSend} aria-label={tr('Send')} title={tr('Send')} onClick={submit}>
            <span aria-hidden="true" className="composer-symbol active">
              <Icon name="arrow.up" size={14} weight="bold" />
            </span>
          </button>
        </div>
      </div>
    </div>
  )
}
