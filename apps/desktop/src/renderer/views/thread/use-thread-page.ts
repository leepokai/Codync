import { useEffect, useRef } from 'react'
import type { Bot } from '@shared/models'
import type { WorkItem } from '@shared/work'
import { usePresence } from '../../components/Overlay'

/** How long the thread's page takes to slide away (`--layout`). */
const SLIDE_MS = 300

/**
 * The thread page of the details panel: `thread` is the item it shows, among `bot`'s. `reading` while it is open,
 * `page` its slide, `shown` the item kept while it slides away, and `opening` remembers the row that opens it, which
 * gets the keyboard back when it closes.
 */
export function useThreadPage(thread: string | null, bot: Bot | null, closeThread: () => void) {
  const item = (thread && bot?.work.find((i) => i.id === thread && i.task)) || null
  // The item left the box (dropped, or its week is over): back to the pages.
  useEffect(() => {
    if (thread && !item) closeThread()
  }, [thread, item, closeThread])
  const page = usePresence(item !== null, SLIDE_MS)
  const shown = useRef<WorkItem | null>(item)
  if (item) shown.current = item
  const opener = useRef<HTMLElement | null>(null)
  const reading = item !== null
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
