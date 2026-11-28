import { useEffect, useMemo, useRef, useState } from 'react'
import { Sheet } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import type { Entry, ReplyQuote } from '@shared/models'

/**
 * Pick a time for "Schedule send…": a new message's, a waiting one's (a time holds it out of the live
 * queue), or the schedule of one already held (`entry`). The input is the browser's local-time picker; the
 * store keeps UTC epoch-ms, so a zone change or DST can't move a message by accident. A time must be at
 * least half a minute out — anything closer is a send, not a schedule.
 */
export function ScheduleSheet({
  botId,
  entry = null,
  draft = '',
  replyTo = null,
  onClose,
  onScheduled,
}: {
  botId: string
  /** A message that hasn't gone out: one waiting in the live queue (a time holds it), or one already held. */
  entry?: Entry | null
  /** New schedule: the draft to hold. */
  draft?: string
  /** New schedule: the message it quotes, as a send would. */
  replyTo?: ReplyQuote | null
  onClose: () => void
  /** New schedule only: the composer's cleanup once the message is scheduled. */
  onScheduled?: () => void
}) {
  const store = useStore()
  const tr = useTr()
  const ref = useRef<HTMLInputElement>(null)
  const held = entry !== null && entry.data.status === 'scheduled'
  const [text, setText] = useState(entry?.data.text ?? draft)
  const [time, setTime] = useState(() => defaultTime(entry?.data.notBefore))
  // A held message's own time is already a pick; a new or waiting one asks for one.
  const [touched, setTouched] = useState(entry?.data.notBefore !== undefined)
  const invalid = useMemo(() => {
    const at = parseLocal(time)
    if (at === null) return tr('Pick a time')
    if (at <= Date.now() + 30_000) return tr('Pick a time at least 30 seconds out')
    return null
  }, [time, tr])
  const ready = text.trim().length > 0 && touched && invalid === null && !store.answering.has(entry?.id ?? '')
  useEffect(() => {
    ref.current?.focus()
  }, [])
  return (
    <Sheet open onClose={onClose}>
      <VStack gap={3} className="schedule-sheet">
        <Text size="headline">{held ? tr('Edit schedule') : tr('Schedule send…')}</Text>
        <textarea
          className="schedule-text"
          value={text}
          placeholder={tr('Message')}
          onChange={(e) => setText(e.target.value)}
        />
        <input
          ref={ref}
          className="schedule-time"
          type="datetime-local"
          aria-label={tr('Schedule for')}
          value={time}
          onChange={(e) => {
            setTouched(true)
            setTime(e.target.value)
          }}
        />
        {invalid ? <Text size="caption" tone="danger">{invalid}</Text> : null}
        <div className="schedule-actions">
          {held ? (
            // Back to an ordinary queued message: it goes out as one typed now would.
            <button
              className="press schedule-cancel"
              disabled={!text.trim()}
              onClick={() => {
                // Clearing the time is a send now: it carries the edit in the sheet, not the saved draft.
                store.sendScheduledNow(entry, text.trim())
                onClose()
              }}
            >
              {tr('Clear the time')}
            </button>
          ) : null}
          <button className="press schedule-cancel" onClick={onClose}>
            {tr('Cancel')}
          </button>
          <button
            className="press schedule-confirm"
            disabled={!ready}
            onClick={() => {
              const at = parseLocal(time)
              if (at === null) return
              if (entry) store.updateScheduled(entry, { text: text.trim(), notBefore: at })
              else {
                store.scheduleSend(text, botId, at, { replyTo })
                onScheduled?.()
              }
              onClose()
            }}
          >
            {held ? tr('Save') : tr('Schedule send')}
          </button>
        </div>
      </VStack>
    </Sheet>
  )
}

/** `now + 1h`, in the `datetime-local` shape, minutes rounded so a fire lands on a clean time. */
function defaultTime(atMs?: number): string {
  const at = atMs !== undefined ? new Date(atMs) : new Date(Date.now() + 60 * 60_000)
  return toLocalInput(at)
}

/** The local-time `datetime-local` value of `at`. */
function toLocalInput(at: Date): string {
  const pad = (n: number, w = 2) => String(n).padStart(w, '0')
  return `${at.getFullYear()}-${pad(at.getMonth() + 1)}-${pad(at.getDate())}T${pad(at.getHours())}:${pad(at.getMinutes())}`
}

/** A `datetime-local` value as epoch-ms, in this machine's timezone; null while incomplete. */
function parseLocal(value: string): number | null {
  if (!/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/.test(value)) return null
  const at = new Date(value).getTime()
  return Number.isFinite(at) ? at : null
}
