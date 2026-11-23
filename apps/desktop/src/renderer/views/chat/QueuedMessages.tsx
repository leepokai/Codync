import { useEffect, useRef, useState } from 'react'
import { hidingCalls } from '@shared/calls'
import { isWorking, quoteOf, type Entry } from '@shared/models'
import { IconButton } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import { usePresence } from '../../components/Overlay'
import { HStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { afterExit } from '../../lib/exit-delay'
import { messagePreview } from '../../lib/message-preview'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import { ComposerReveal } from './Composer'
import { canSchedule, canSteer, waitingMessages } from './queued-messages'
import { ScheduleSheet } from './ScheduleSheet'
import './queued-messages.css'
import './schedule-sheet.css'

const EXIT_MS = 300

/**
 * The user's pending messages in a tray tucked under the composer's top edge: the live queue (while the bot works,
 * oldest first — each can go out now (`steer`) or be taken back (`unqueue`)) and the scheduled ones (a clock icon,
 * their fire time, editable and cancelable). A host older than steering only lists them.
 */
export function QueuedMessages({ botId }: { botId: string }) {
  const store = useStore()
  const bot = store.bots.get(botId)
  const waiting = waitingMessages(hidingCalls(store.allEntries(botId)), !!bot && isWorking(bot))
  const rows = useLeaving(waiting)
  const list = useRevealSent(waiting.at(-1), store.lastSentNonce)
  const tray = usePresence(waiting.length > 0, EXIT_MS)
  const actions = canSteer(store.hostVersion?.version)
  const scheduling = canSchedule(store.hello)
  const [editing, setEditing] = useState<Entry | null>(null)
  if (!tray.mounted) return null
  return (
    <div className={`queued-tray ${tray.shown ? 'shown' : ''}`} inert={waiting.length === 0}>
      <div>
        <div ref={list} className="queued-list" role="list" aria-label="Queued messages">
          {rows.map(({ entry, leaving }) => (
            <QueuedRow
              key={entry.id}
              entry={entry}
              leaving={leaving}
              actions={actions}
              scheduling={scheduling}
              onEdit={() => setEditing(entry)}
            />
          ))}
        </div>
      </div>
      {editing ? (
        <ScheduleSheet
          botId={botId}
          entry={editing}
          onClose={() => setEditing(null)}
        />
      ) : null}
    </div>
  )
}

function QueuedRow({
  entry,
  leaving,
  actions,
  scheduling,
  onEdit,
}: {
  entry: Entry
  leaving: boolean
  actions: boolean
  scheduling: boolean
  onEdit: () => void
}) {
  const store = useStore()
  const tr = useTr()
  const answering = store.answering.has(entry.id)
  const fireAt = entry.data.notBefore
  const text = quoteOf(entry).text
  // Click shows the whole message; a second click folds it back; a double-click edits it in place.
  const [expanded, setExpanded] = useState(false)
  const [editing, setEditing] = useState(false)
  const [draft, setDraft] = useState(text)
  const startEdit = () => {
    setDraft(text)
    setEditing(true)
  }
  const commit = () => {
    setEditing(false)
    const trimmed = draft.trim()
    if (trimmed && trimmed !== text) store.updateScheduled(entry, { text: trimmed })
  }
  return (
    <div className="queued-row" role="listitem">
      <ComposerReveal open={!leaving}>
        {editing ? (
          <input
            className="queued-edit"
            autoFocus
            value={draft}
            aria-label={tr('Edit')}
            onChange={(e) => setDraft(e.target.value)}
            onBlur={commit}
            onKeyDown={(e) => {
              if (e.key === 'Enter') commit()
              if (e.key === 'Escape') setEditing(false)
            }}
          />
        ) : (
          <HStack gap={2} className="queued-row-body">
            {scheduling ? (
              // The row's one time control: it sets a waiting message's time, edits a held one's.
              <IconButton
                title={tr(fireAt === undefined ? 'Schedule send…' : 'Edit schedule')}
                icon="clock"
                size={26}
                disabled={answering}
                onClick={onEdit}
              />
            ) : fireAt !== undefined ? (
              <span className="queued-scheduled" title={tr('Scheduled')}>
                <Icon name="clock" size={11} color="var(--secondary)" />
              </span>
            ) : null}
            <Text
              size="callout"
              tone="secondary"
              truncate={!expanded}
              grow
              className="queued-preview"
              onClick={() => setExpanded(!expanded)}
              onDoubleClick={startEdit}
            >
              {expanded ? text : messagePreview(text)}
              {fireAt !== undefined ? (
                <span className="queued-fire-time">
                  {' '}
                  · {tr('Fires {time}').replaceAll('{time}', () => new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(fireAt)))}
                </span>
              ) : null}
            </Text>
            <IconButton title={tr('Copy')} icon="doc.on.doc" size={26} onClick={() => window.codync.app.copy(text)} />
            <IconButton title={tr('Edit')} icon="pencil" size={26} disabled={answering} onClick={startEdit} />
            {actions && fireAt === undefined ? (
              <>
                <IconButton title="Send now" icon="arrow.turn.down.right" size={26} disabled={answering} onClick={() => store.steer(entry)} />
                <IconButton title="Remove" icon="trash" size={26} disabled={answering} onClick={() => store.unqueue(entry)} />
              </>
            ) : null}
            {scheduling && fireAt !== undefined ? (
              <>
                <IconButton title={tr('Send now')} icon="arrow.turn.down.right" size={26} disabled={answering} onClick={() => store.sendScheduledNow(entry)} />
                <IconButton title={tr('Remove')} icon="trash" size={26} disabled={answering} onClick={() => store.cancelScheduled(entry)} />
              </>
            ) : null}
          </HStack>
        )}
      </ComposerReveal>
    </div>
  )
}

/** The list stays where it is, the oldest (next to go) in view; a message sent from here shows that it arrived. */
function useRevealSent(newest: Entry | undefined, lastSentNonce: string | null) {
  const list = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (!newest?.data.clientNonce || newest.data.clientNonce !== lastSentNonce) return
    // Once its row has opened, so the scroll takes it in.
    return afterExit(EXIT_MS, () => list.current?.scrollTo({ top: list.current.scrollHeight, behavior: 'smooth' }))
  }, [newest?.id])
  return list
}

/** `items` plus the ones that just left, kept in their places until their row has folded away. */
function useLeaving(items: Entry[]): { entry: Entry; leaving: boolean }[] {
  const [previous, setPrevious] = useState(items)
  const [leaving, setLeaving] = useState<{ entry: Entry; index: number }[]>([])
  const keys = items.map((e) => e.id)
  // Worked out while rendering, so a row that leaves never unmounts before it folds away.
  if (keys.join('\n') !== previous.map((e) => e.id).join('\n')) {
    const gone = previous.flatMap((entry, index) => (keys.includes(entry.id) ? [] : [{ entry, index }]))
    setPrevious(items)
    setLeaving((list) => [...list.filter((x) => !keys.includes(x.entry.id)), ...gone])
  }
  useEffect(() => (leaving.length ? afterExit(EXIT_MS, () => setLeaving([])) : undefined), [leaving])
  const rows = items.map((entry) => ({ entry, leaving: false }))
  for (const { entry, index } of leaving) {
    if (!keys.includes(entry.id)) rows.splice(Math.min(index, rows.length), 0, { entry, leaving: true })
  }
  return rows
}
