import { useEffect, useMemo, useRef, useState } from 'react'
import { Sheet } from '../../components/Overlay'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { useStore } from '../../store/context'
import type { Entry } from '@shared/models'

/**
 * Pick a time for "Schedule send…": a new message's, or the schedule of one already waiting (`entry`). The
 * input is the browser's local-time picker; the store keeps UTC epoch-ms, so a zone change or DST can't
 * move a message by accident.
 */
export function ScheduleSheet({
  botId,
  entry = null,
  draft = '',
  onClose,
  onScheduled,
}: {
  botId: string
  /** Editing: the scheduled entry (its text and time prefill, saving updates it in place). */
  entry?: Entry | null
  /** New schedule: the draft to hold. */
  draft?: string
  onClose: () => void
  /** New schedule only: the composer's cleanup once the message is scheduled. */
  onScheduled?: () => void
}) {
  const store = useStore()
  const tr = useTr()
  const ref = useRef<HTMLInputElement>(null)
  const editing = entry !== null
  const [text, setText] = useState(entry?.data.text ?? draft)
  const [time, setTime] = useState(() => defaultTime(entry?.data.notBefore))
  // A local-time value is only a value once a field was typed into; the picker starts blank.
  const [touched, setTouched] = useState(false)
  const invalid = useMemo(() => {
    const at = parseLocal(time)
    if (at === null) return tr('Pick a time')
    if (!editing && at <= Date.now()) return tr('Pick a time in the future')
    if (editing && at <= Date.now()) return tr('Pick a time in the future')
    return null
  }, [time, editing, tr])
  const ready = text.trim().length > 0 && touched && invalid === null && !store.answering.has(entry?.id ?? '')
  useEffect(() => {
    ref.current?.focus()
  }, [])
  return (
    <Sheet open onClose={onClose}>
      <VStack gap={3} className="schedule-sheet">
        <Text size="headline">{editing ? tr('Edit schedule') : tr('Schedule send')}</Text>
        <textarea
          className="schedule-text"
          value={text}
          readOnly={editing}
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
          <button className="press schedule-cancel" onClick={onClose}>
            {tr('Cancel')}
          </button>
          <button
            className="press schedule-confirm"
            disabled={!ready}
            onClick={() => {
              const at = parseLocal(time)
              if (at === null) return
              if (editing) store.updateScheduled(entry, { text: text.trim(), notBefore: at })
              else {
                store.scheduleSend(text, botId, at, { replyTo: null })
                onScheduled?.()
              }
              onClose()
            }}
          >
            {editing ? tr('Save') : tr('Schedule')}
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
