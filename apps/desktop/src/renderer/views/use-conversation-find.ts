import { useCallback, useEffect, useRef, useState } from 'react'
import type { FindState } from '../lib/find-label'
import { clearPaint, findRanges, paint, reveal } from '../lib/transcript-find'
import { analytics } from '../store/analytics'

const NONE: FindState = { active: 0, matches: 0 }
/** After this long without typing, a search counts as made. */
const SETTLE_MS = 700
/** New messages and streaming text change the page in bursts: look again once they pause. */
const REFRESH_MS = 200

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

  const search = useCallback(
    (text: string, scroll: boolean) => {
      const element = rootRef.current()
      ranges.current = element ? findRanges(element, text) : []
      show(scroll)
    },
    [show],
  )

  // A new text starts from its first match.
  useEffect(() => {
    active.current = 0
    search(query, true)
  }, [query, search])

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
      timer = setTimeout(() => search(query, false), REFRESH_MS)
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
