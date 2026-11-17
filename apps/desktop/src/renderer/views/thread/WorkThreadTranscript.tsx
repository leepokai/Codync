import { useEffect, useRef, useState } from 'react'
import type { Entry } from '@shared/models'
import { isTold } from '@shared/tasks'
import { isLive, isThreadMessage, sharedLine, threadEntries, type WorkItem } from '@shared/work'
import { Button, IconButton, Reveal, Spinner } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, Spacer, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'
import type { ThreadRef } from '../../store/bot-store'
import { useStore } from '../../store/context'
import { AgentBubble, Bubble } from './ChatRows'
import { TraceRow } from './TraceView'
import { runningTurn } from './trace-turns'
import { copyMessageText } from './use-image-actions'
import './trace.css'
import './work.css'

/**
 * A thread's transcript: what the task was told, folded at the top, then its steps and messages in order (the
 * user's own and the task's replies as the chat's bubbles), with one quiet line where the task put something in the
 * bot's chat (a card, a file, a voice note, an artifact), which shows that entry in the chat (`showInChat`). Fetched
 * as it opens and kept current while the task works.
 * ponytail: only among the chat's shown messages (a needs-you card pages back to its own); load older pages, as the
 * card does, if an old item's thread misses what it shared.
 */
export function WorkThreadTranscript({ thread, item, openArtifact, showInChat }: { thread: ThreadRef; item: WorkItem; openArtifact: (artifactId: string) => void; showInChat: (entryId: string) => void }) {
  const tr = useTr()
  const store = useStore()
  const taskId = item.task ?? ''
  const live = isLive(item)
  const [attempt, setAttempt] = useState(0)
  // Read again when the link comes back: its catch-up carries no task entries. Not as it drops: a read wakes the link.
  const online = store.connection.kind === 'online'
  const wasOnline = useRef(online)
  useEffect(() => {
    if (online && !wasOnline.current) setAttempt((n) => n + 1)
    wasOnline.current = online
  }, [online])
  // Read again once it ends, for how it ended.
  const { fetched, error } = useFetched(() => store.loadTask(thread.botId, taskId), [store, thread.botId, taskId, live, attempt])
  const entries = store.taskEntriesOf(taskId)
  const { told, rest } = threadEntries(entries, store.allEntries(thread.botId), taskId)
  const running = runningTurn(entries, live)
  // The latest attempt's outcome: a read that failed shows until the next one answers.
  const failed = error !== null && fetched === null
  return (
    <VStack gap={10} className="work-thread-content">
      {told ? <Told entry={told} /> : null}
      {failed ? (
        <VStack gap={10} align="center" className="work-thread-note">
          <Text size="footnote" tone="danger">{error}</Text>
          <Button kind="secondary" onClick={() => setAttempt((n) => n + 1)}>Try again</Button>
        </VStack>
      ) : null}
      {!failed && fetched === null && entries.length === 0 ? <div className="work-thread-note" aria-label={tr('Loading')}><Spinner /></div> : null}
      {!failed && fetched !== null && entries.length === 0 ? <Text as="div" size="footnote" tone="tertiary" className="work-thread-note">{tr('Nothing yet.')}</Text> : null}
      {rest.map((entry) => (
        <Row key={entry.id} entry={entry} thread={thread} unfolded={entry.turn === running} openArtifact={openArtifact} showInChat={showInChat} />
      ))}
    </VStack>
  )
}

/** What the host told the task (its item and brief), folded under one line. */
function Told({ entry }: { entry: Entry }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  return (
    <div className="work-thread-told">
      <button className="disclosure-label work-thread-told-label" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <Text size="footnote" weight="semibold" tone="text">{tr('What it was told')}</Text>
        <Spacer min={8} />
        <span className={`disclosure-chevron${open ? ' open' : ''}`}>
          <Icon name="chevron.right" size={10} weight="semibold" />
        </span>
      </button>
      <Reveal show={open}>
        <Text as="div" size="footnote" tone="secondary" className="selectable work-thread-told-text">{entry.data.text ?? ''}</Text>
      </Reveal>
    </div>
  )
}

function Row({ entry, thread, unfolded, openArtifact, showInChat }: { entry: Entry; thread: ThreadRef; unfolded: boolean; openArtifact: (artifactId: string) => void; showInChat: (entryId: string) => void }) {
  // What the task put in the bot's chat stays there, answered and downloaded by its own id: here, one line to it.
  if (entry.botId === thread.botId) return <Shared entry={entry} show={() => showInChat(entry.id)} />
  // What the host told it later (a new brief) reads as a quote, not as something the user typed.
  if (isTold(entry)) return <Text as="div" size="footnote" tone="secondary" className="selectable work-thread-quote">{entry.data.text ?? ''}</Text>
  if (entry.kind === 'user') return <Mine entry={entry} thread={thread} />
  if (isThreadMessage(entry)) return <AgentBubble entry={entry} plain openArtifact={openArtifact} />
  return <TraceRow entry={entry} unfolded={unfolded} />
}

/** The line for an entry of the bot's chat: its symbol and what it is (`sharedLine`), a button to it in the chat. */
function Shared({ entry, show }: { entry: Entry; show: () => void }) {
  const tr = useTr()
  const line = sharedLine(entry, tr)
  if (!line) return null
  return (
    <button className="work-thread-shared" title={tr('Show in the chat')} onClick={show}>
      <HStack as="span" gap={6}>
        <Icon name={line.symbol} size={11} color="var(--secondary)" />
        <Text size="footnote" tone="secondary" truncate grow>{line.text}</Text>
      </HStack>
    </button>
  )
}

/** One of the user's messages to the thread: the chat's bubble, with what can be done while it waits or failed. */
function Mine({ entry, thread }: { entry: Entry; thread: ThreadRef }) {
  const status = entry.data.status
  return (
    <VStack align="flex-end" gap={4} className={status === 'queued' ? 'work-thread-mine queued' : 'work-thread-mine'} data-message-status={status}>
      <Bubble className="bubble user selectable" items={() => [{ title: 'Copy', icon: 'square.on.square', action: () => copyMessageText(entry) }]}>
        {entry.data.text}
      </Bubble>
      <MineStatus entry={entry} thread={thread} />
    </VStack>
  )
}

/** Under a message that isn't simply sent: it waits behind the task's turn (Send now, Remove), is on its way, didn't
 * arrive (Resend, Delete), or was dropped by Stop. */
function MineStatus({ entry, thread }: { entry: Entry; thread: ThreadRef }) {
  const tr = useTr()
  const store = useStore()
  const busy = store.answering.has(entry.id) || store.isOffline
  switch (entry.data.status) {
    case 'queued':
      return (
        <HStack as="span" gap={2}>
          <Text size="caption2" tone="tertiary" className="work-thread-mine-status">{tr('Queued')}</Text>
          <IconButton title="Send now" icon="arrow.turn.down.right" size={24} disabled={busy} onClick={() => store.steer(entry, thread)} />
          <IconButton title="Remove" icon="trash" size={24} disabled={busy} onClick={() => store.unqueue(entry, thread)} />
        </HStack>
      )
    case 'sending':
      return <Text size="caption2" tone="tertiary">{tr('Sending…')}</Text>
    case 'failed':
      return (
        <HStack as="span" gap={2}>
          <Text size="caption2" weight="semibold" tone="danger" className="work-thread-mine-status">{tr('Failed to send')}</Text>
          {/* Only one that never arrived can go again; one the host failed (a restart) is its record. */}
          {entry.rev === 0 && entry.data.clientNonce ? (
            <>
              <IconButton title="Resend" icon="arrow.clockwise" size={24} disabled={store.isOffline} onClick={() => store.sendToThread(thread, entry.botId, entry.data.text ?? '', entry.data.clientNonce)} />
              <IconButton title="Delete" icon="trash" size={24} onClick={() => store.discardFromThread(entry)} />
            </>
          ) : null}
        </HStack>
      )
    case 'cancelled':
      return <Text size="caption2" tone="tertiary">{tr('Not sent — stopped')}</Text>
    default:
      return null
  }
}
