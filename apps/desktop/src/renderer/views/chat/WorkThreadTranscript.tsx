import { useEffect, useMemo, useRef, useState } from 'react'
import type { Entry } from '@shared/models'
import { isLive, threadEntries, type WorkTask } from '@shared/work'
import { Button, IconButton, Reveal } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useFetched } from '../../lib/use-fetched'
import { useTr } from '../../lib/use-language'
import type { ThreadRef } from '../../store/bot-store'
import { useStore } from '../../store/context'
import { AttachmentList } from './Attachments'
import { Bubble } from './ChatRows'
import { copyMessageText } from './use-image-actions'
import './work.css'

/**
 * A thread's transcript from the computer, read as it opens and kept current while its task works: the task's own
 * `entries`, and of them what it was `told` first and the `rest` in order with what it put in the bot's chat
 * (`threadEntries`). `fetched`: the computer has answered once; `failed`: its latest answer was an `error`.
 */
export function useThreadEntries(thread: ThreadRef, task: WorkTask) {
  const store = useStore()
  const threadId = task.thread ?? ''
  const live = isLive(task)
  const [attempt, setAttempt] = useState(0)
  // Read again when the link comes back: its catch-up carries no thread entries. Not as it drops: a read wakes the link.
  const online = store.connection.kind === 'online'
  const wasOnline = useRef(online)
  useEffect(() => {
    if (online && !wasOnline.current) setAttempt((n) => n + 1)
    wasOnline.current = online
  }, [online])
  // Read again once it ends, for how it ended.
  const { fetched, error } = useFetched(() => store.loadThread(thread.botId, threadId), [store, thread.botId, threadId, live, attempt])
  const entries = store.threadEntriesOf(threadId)
  const chat = store.allEntries(thread.botId)
  const { told, rest } = useMemo(() => threadEntries(entries, chat, threadId), [entries, chat, threadId])
  // The latest attempt's outcome: a read that failed shows until the next one answers.
  const failed = error !== null && fetched === null
  return { entries, told, rest, fetched: fetched !== null, failed, error, retry: () => setAttempt((n) => n + 1) }
}

/** A thread that couldn't be read, and a button to read it again. */
export function ThreadReadFailed({ error, retry }: { error: string | null; retry: () => void }) {
  return (
    <VStack gap={10} align="center" className="work-thread-note">
      <Text size="footnote" tone="danger">{error}</Text>
      <Button kind="secondary" onClick={retry}>Try again</Button>
    </VStack>
  )
}

/** What the host told the thread (its task and brief), folded under one line. */
export function Told({ entry }: { entry: Entry }) {
  const tr = useTr()
  const [open, setOpen] = useState(false)
  return (
    <div className="work-thread-told">
      <button className="work-thread-told-label" aria-expanded={open} onClick={() => setOpen((o) => !o)}>
        <Text size="subheadline" tone="secondary">{tr('What it was told')}</Text>
        <span className={`disclosure-chevron${open ? ' open' : ''}`}>
          <Icon name="chevron.right" size={8} weight="semibold" />
        </span>
      </button>
      <Reveal show={open}>
        <Text as="div" size="footnote" tone="secondary" className="selectable work-thread-told-text">{entry.data.text ?? ''}</Text>
      </Reveal>
    </div>
  )
}

/** One of the user's messages to the thread: the chat's bubble, with what can be done while it waits or failed. */
export function Mine({ entry, thread }: { entry: Entry; thread: ThreadRef }) {
  const status = entry.data.status
  return (
    <VStack align="flex-end" gap={4} className={status === 'queued' ? 'work-thread-mine queued' : 'work-thread-mine'} data-message-status={status}>
      {entry.data.attachments?.length ? <AttachmentList attachments={entry.data.attachments} botId={entry.botId} /> : null}
      {entry.data.text ? (
        <Bubble className="bubble user selectable" items={() => [{ title: 'Copy', icon: 'square.on.square', action: () => copyMessageText(entry) }]}>
          {entry.data.text}
        </Bubble>
      ) : null}
      <MineStatus entry={entry} thread={thread} />
    </VStack>
  )
}

/** Under a message that isn't simply sent: it waits behind the thread's turn (Send now, Remove), is on its way, didn't
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
              <IconButton title="Resend" icon="arrow.clockwise" size={24} disabled={store.isOffline} onClick={() => store.resendToThread(thread, entry)} />
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
