import { useCallback, useEffect, useRef, useState } from 'react'
import type { FindState } from '../lib/find-label'
import { keptMatch } from '../lib/find-position'
import { clearPaint, findRanges, paint, reveal } from '../lib/transcript-find'
import { analytics } from '../store/analytics'

const NONE: FindState = { active: 0, matches: 0 }
/** After this long without typing, a search counts as made. */
const SETTLE_MS = 700
/** New messages and streaming text change the page in bursts: look again once they pause. */
const REFRESH_MS = 200

/** Which of two matches starts first in the page. */
const startOrder = (a: Range, b: Range) => a.compareBoundaryPoints(Range.START_TO_START, b)

/**
 * Find in the open conversation (⌘F): the matches of `query` in the transcript, the current one, and stepping
 * between them. The view draws the bar; the page's own highlights mark the matches.
 */
export function useConversationFind(root: () => HTMLElement | null) {
  const [query, setQuery] = useState('')
  const [state, setState] = useState<FindState>(NONE)
  const ranges = useRef<Range[]>([])
  const active = useRef(0)
  const reported = useRef('')
  const rootRef = useRef(root)
  rootRef.current = root

  const show = useCallback((scroll: boolean) => {
    const found = ranges.current
    active.current = Math.min(active.current, Math.max(found.length - 1, 0))
    if (found.length === 0) clearPaint()
    else paint(found, active.current)
    setState({ active: found.length === 0 ? 0 : active.current + 1, matches: found.length })
    const current = found[active.current]
    if (scroll && current) reveal(current)
  }, [])

  /** Searches the transcript again: from the first match for a new text, or staying on the current one as the page changes. */
  const search = useCallback(
    (text: string, from: 'first' | 'current') => {
      const element = rootRef.current()
      const previous = ranges.current[active.current]
      ranges.current = element ? findRanges(element, text) : []
      if (from === 'first') active.current = 0
      // Page ranges are live: the current one still marks the same text after messages went in above it.
      else if (previous?.startContainer.isConnected) active.current = keptMatch(ranges.current, previous, startOrder)
      show(from === 'first')
    },
    [show],
  )

  useEffect(() => search(query, 'first'), [query, search])

  // Counted once per text searched, when typing pauses or a step is taken, as whether it found anything; never the text.
  const report = useCallback(() => {
    if (!query.trim() || reported.current === query) return
    reported.current = query
    analytics.track({ event: 'conversation_find_searched', found: ranges.current.length > 0 })
  }, [query])
  useEffect(() => {
    const timer = setTimeout(report, SETTLE_MS)
    return () => clearTimeout(timer)
  }, [report])

  // The conversation keeps changing while the bar is open.
  useEffect(() => {
    const element = rootRef.current()
    if (!element) return
    let timer: ReturnType<typeof setTimeout> | undefined
    const observer = new MutationObserver(() => {
      clearTimeout(timer)
      timer = setTimeout(() => search(query, 'current'), REFRESH_MS)
    })
    observer.observe(element, { subtree: true, childList: true, characterData: true })
    return () => {
      clearTimeout(timer)
      observer.disconnect()
    }
  }, [query, search])

  useEffect(() => clearPaint, [])

  const step = useCallback(
    (forward: boolean) => {
      const count = ranges.current.length
      if (count === 0) return
      active.current = (active.current + (forward ? 1 : -1) + count) % count
      show(true)
      report()
    },
    [show, report],
  )

  return { query, setQuery, state, step }
}

/**
 * Whether the find bar is open over the conversation `key`: its focus count while open (⌘F again bumps it, putting the
 * cursor back in the field), null while closed. Another conversation closes it; opening it is counted once per bar.
 */
export function useFindBar(key: string | null) {
  const [focus, setFocus] = useState<number | null>(null)
  const isOpen = useRef(false)
  const open = useCallback(() => {
    if (!isOpen.current) analytics.track({ event: 'conversation_find_opened' })
    isOpen.current = true
    setFocus((n) => (n ?? 0) + 1)
  }, [])
  const close = useCallback(() => {
    isOpen.current = false
    setFocus(null)
  }, [])
  useEffect(close, [key, close])
  return { focus, open, close }
}
