import { useEffect, useRef } from 'react'
import type { Bot } from '@shared/models'
import type { WorkTask } from '@shared/work'
import { usePresence } from '../../components/Overlay'

/** How long the thread's page takes to slide away (`--layout`). */
const SLIDE_MS = 300

/**
 * The thread page of the details panel: `thread` is the task it shows, among `bot`'s. `reading` while it is open,
 * `page` its slide, `shown` the task kept while it slides away, and `opening` remembers the row that opens it, which
 * gets the keyboard back when it closes.
 */
export function useThreadPage(thread: string | null, bot: Bot | null, closeThread: () => void) {
  const task = (thread && bot?.work.find((t) => t.id === thread && t.thread)) || null
  // The task left the box (dropped, or its week is over): back to the pages.
  useEffect(() => {
    if (thread && !task) closeThread()
  }, [thread, task, closeThread])
  const page = usePresence(task !== null, SLIDE_MS)
  const shown = useRef<WorkTask | null>(task)
  if (task) shown.current = task
  const opener = useRef<HTMLElement | null>(null)
  const reading = task !== null
  useEffect(() => {
    if (reading) return
    opener.current?.focus({ preventScroll: true })
    opener.current = null
  }, [reading])
  const opening = () => {
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
  }
  return { reading, page, shown: shown.current, opening }
}
