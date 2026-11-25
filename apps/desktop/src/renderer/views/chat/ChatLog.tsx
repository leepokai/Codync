import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { Spinner } from '../../components/Controls'
import { usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'
import { keptPlace, placesOf, contentTopOf, rowsCanScroll, shiftTo, type Place } from '../../lib/scroll-anchor'
import { entryFrom, indexOfEntry, pageAbove, startShowing, windowStart, type WindowStart } from '../../lib/shown-window'
import { rowIn, useArrivals, type ChatItem } from './chat-items'
import { MacChatButton } from './MacChatButton'

const PAGE = 40

/** The message box floats over the rows: its height is the room they leave under the last one. */
export function useDockHeight(root: RefObject<HTMLElement | null>, dock: RefObject<HTMLElement | null>) {
  useLayoutEffect(() => {
    const el = root.current
    if (!el || !dock.current) return
    const docked = new ResizeObserver(([e]) => el.style.setProperty('--composer-height', `${Math.ceil(e!.borderBoxSize[0]!.blockSize)}px`))
    docked.observe(dock.current)
    return () => docked.disconnect()
  }, [])
}

/**
 * A conversation's rows, a chat's or a thread's: the newest page first and earlier ones as the reader nears the top,
 * brought to the newest as it changes while the reader is at the bottom.
 */
export function ChatLog({ items: all, fresh, canLoadOlder, loadOlder, revealed = null, intro, row, working }: {
  /** Every loaded row, oldest first: a new list only when the conversation changes. */
  items: ChatItem[]
  /** That list as it is now, read after waiting on the computer. */
  fresh: () => ChatItem[]
  /** The computer has rows before the loaded ones. */
  canLoadOlder: boolean
  loadOlder: () => Promise<unknown>
  /** A loaded entry to bring into the middle, a new object for each asking. One that can't be found leaves the rows
   *  where they were. */
  revealed?: { entryId: string } | null
  /** In the rows' place while there are none. */
  intro?: ReactNode
  row: (item: ChatItem) => ReactNode
  /** The working line, under the rows. */
  working?: ReactNode
}) {
  const tr = useTr()
  // Where the shown rows start in the loaded list; null is the newest PAGE rows. A number, not an id: the ids of
  // rows are derived from the list they were built in, so they cannot say where a reader was (shown-window.ts).
  const [firstShown, setFirstShown] = useState<WindowStart>(null)
  const [loadingEarlier, setLoadingEarlier] = useState(false)
  const start = windowStart(all.length, firstShown, PAGE)
  const items = all.slice(start)
  const more = start > 0 || canLoadOlder
  // Following the newest message, or reading history: scrolling up releases the bottom,
  // arriving back at the end (or Jump to latest) restores it. Content growth alone never changes it.
  const [following, setFollowingState] = useState(true)
  // Set with the state, not from it: a store change renders before a pending change to it, and would undo it.
  const followingRef = useRef(true)
  const setFollowing = (on: boolean) => {
    followingRef.current = on
    setFollowingState(on)
  }
  const scroller = useRef<HTMLDivElement>(null)
  const lastScrollTop = useRef(0)
  const keepPlace = useRef<Place | null>(null)
  const last = items[items.length - 1]
  const jump = usePresence(!following && items.length > 0, 200)
  const arrivals = useArrivals(items.map((i) => i.id), following)

  const toBottom = (smooth = false) => {
    const el = scroller.current
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' })
  }

  // `revealed`: the shown rows reach back to its entry, and it scrolls into the middle.
  const [revealing, setRevealing] = useState<{ entryId: string } | null>(null)
  useEffect(() => {
    if (!revealed) return
    const list = fresh()
    setFirstShown((from) => startShowing(list, revealed.entryId, from, PAGE))
    setRevealing({ entryId: revealed.entryId })
  }, [revealed])
  // At once, not smooth: a smooth scroll still at the end lets the follower take the chat back down. Brought up from
  // the newest, the reader stops following it.
  useEffect(() => {
    const el = scroller.current
    if (!el || !revealing) return
    el.querySelector(`[data-entry-id="${CSS.escape(revealing.entryId)}"]`)?.scrollIntoView({ block: 'center' })
    if (el.scrollHeight - el.scrollTop - el.clientHeight >= 8) setFollowing(false)
  }, [revealing])

  const showEarlier = async () => {
    if (loadingEarlier) return
    setLoadingEarlier(true)
    const el = scroller.current
    // The row the reader is on, kept for however many rows go in above it. A reader at the newest keeps none: the
    // rows are brought to the bottom after every render there, and a place would only fight that.
    keepPlace.current = el ? keptPlace(placesOf(el), followingRef.current) : null
    // The entry the reader's first row is about, which is what the rebuilt list is asked for. Not the first row's
    // own id: that is often a date separator, and the separator stops existing as soon as rows above it arrive.
    const wasShowing = entryFrom(all, start)
    if (canLoadOlder) await loadOlder()
    // Read after the wait: the rows that went in are above everything held, so the row the reader is on sits that
    // many rows further down. With nothing added, the rows themselves are already held and one page of them is all
    // there is to show.
    const list = fresh()
    const above = indexOfEntry(list, wasShowing)
    // The reader's own entry is no longer in the list at all: leave the window where it is rather than move it to a
    // page of the conversation the reader was not reading.
    if (above === null) {
      setLoadingEarlier(false)
      return
    }
    setFirstShown(pageAbove(above, PAGE))
    setLoadingEarlier(false)
  }

  // Rows going in above keep the reader's place; a follower stays at the bottom. This runs after every render, so the
  // place has to survive whatever renders come first: it moves the view only by how far the row itself moved in the
  // content, which is zero for a message arriving below, for the box growing, and for the fetch's own "loading"
  // render — and it is dropped once nothing more can arrive.
  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    const place = keepPlace.current
    if (place && !followingRef.current) {
      const row = el.querySelector<HTMLElement>(`[data-entry-id="${CSS.escape(place.id)}"]`)
      // Undone by geometry, not by an id: the row's start in the content is the same number however the view is
      // scrolled, so applying the shift restores it exactly and applying it twice changes nothing.
      const moved = row ? shiftTo(place, contentTopOf(el, row)) : 0
      if (moved !== 0) {
        el.scrollTop += moved
        lastScrollTop.current = el.scrollTop
      }
      // Nothing to keep a place on, or the arrival has been paid for and no more rows can come: drop it, so a later
      // insertion above this row (with the reader long gone from it) is not paid for either.
      if (!row || (moved === 0 && !loadingEarlier)) keepPlace.current = null
    } else if (followingRef.current) {
      // Cleared here too: a place that outlived the render which took it would move a reader who has since left
      // the bottom, and the branch above would never see it again.
      keepPlace.current = null
      toBottom()
    }
  })

  // A message the user sends brings the chat back to the newest.
  const lastIsUser = last?.kind === 'entry' && last.entry.kind === 'user'
  useEffect(() => {
    if (lastIsUser) setFollowing(true)
  }, [last?.id, lastIsUser])

  // Rows that grow after layout (images, the text size) keep a follower at the bottom.
  useEffect(() => {
    const el = scroller.current
    if (!el) return
    const observer = new ResizeObserver(() => {
      if (followingRef.current) toBottom()
    })
    observer.observe(el.firstElementChild!)
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  return (
    <div className="transcript-frame">
      <div
        className="transcript"
        ref={scroller}
        role="log"
        data-find-root
        aria-label={tr('Messages')}
        onScroll={(e) => {
          const el = e.currentTarget
          const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < 8
          if (el.scrollTop < lastScrollTop.current - 1 && !atEnd) setFollowing(false)
          else if (atEnd) setFollowing(true)
          lastScrollTop.current = el.scrollTop
          // Rows shorter than the view sit at their top permanently, where "near the top" says nothing: there are
          // no earlier rows to bring in, and asking would take a place from a view that never moved.
          if (more && rowsCanScroll(el.scrollHeight, el.clientHeight) && el.scrollTop < 200 && !loadingEarlier) void showEarlier()
        }}
      >
        <div className="transcript-content">
          {more ? (
            <button className="load-earlier" disabled={loadingEarlier} onClick={() => void showEarlier()}>
              {loadingEarlier ? <Spinner size={14} /> : tr('Load earlier messages')}
            </button>
          ) : null}
          {items.length === 0 && intro ? <div className="transcript-intro">{intro}</div> : null}
          {items.map((item) => (
            // `data-entry-id`: what a quote's click looks for (a user row's `item.id` is its nonce).
            <div key={item.id} className={arrivals.has(item.id) ? rowIn(item) : undefined} data-entry-id={item.kind === 'separator' ? undefined : item.entry.id}>
              {row(item)}
            </div>
          ))}
          {working}
          <div className="chat-end" />
        </div>
      </div>
      {jump.mounted ? (
        <div className={`jump-to-latest ${jump.shown ? 'shown' : ''}`} inert={!jump.shown}>
          <MacChatButton
            title="Jump to latest"
            icon="arrow.down"
            direction={[0, 1]}
            onClick={() => {
              setFollowing(true)
              toBottom(true)
            }}
          />
        </div>
      ) : null}
    </div>
  )
}
