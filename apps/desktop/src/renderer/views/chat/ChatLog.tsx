import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { Spinner } from '../../components/Controls'
import { usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'
import { keptPlace, placesOf, contentTopOf, rowsCanScroll, settledPlace, shiftTo, type Place } from '../../lib/scroll-anchor'
import { entryFrom, pagePlan, pinnedWindowStart, revealWindowStart, windowAnchors, type WindowStart } from '../../lib/shown-window'
import { rowIdentity, rowIn, useArrivals, type ChatItem } from './chat-items'
import { MacChatButton } from './MacChatButton'

const PAGE = 40

/** The row showing `id`: its own entry, or one inside a grouped row whose primary entry is another. */
function rowShowing(el: HTMLElement, id: string): HTMLElement | null {
  return [...el.querySelectorAll<HTMLElement>('[data-entry-ids]')].find((row) => row.dataset.entryIds?.split(' ').includes(id)) ?? null
}

/** A visible row as the reader sees it: every id it can be found by, and its offset below the view's top edge. */
type ReadingRow = { ids: string[]; offset: number }

/** Every id a rendered row answers to: its own row id, then every host entry it shows. */
function idsOf(row: HTMLElement): string[] {
  return [row.dataset.rowId, ...(row.dataset.entryIds?.split(' ') ?? [])].filter((id): id is string => !!id)
}

/** The first rendered row showing one of `ids`, found after a group's primary entry changed. */
function rowAmong(el: HTMLElement, ids: string[]): HTMLElement | null {
  const wanted = new Set(ids)
  return [...el.querySelectorAll<HTMLElement>('[data-row-id]')].find((row) => idsOf(row).some((id) => wanted.has(id))) ?? null
}

/**
 * The rows in front of the reader: the row at the top edge and every row crossing the rest of the view. The first is
 * the one a deletion above the view moves; the others let a later render recover when that row itself goes away.
 */
function readingRowsOf(el: HTMLElement): ReadingRow[] {
  const viewTop = el.getBoundingClientRect().top
  const viewBottom = viewTop + el.clientHeight
  const rows = [...el.querySelectorAll<HTMLElement>('[data-row-id]')]
  const first = rows.findIndex((row) => row.getBoundingClientRect().bottom > viewTop + 2)
  if (first < 0) return []
  return rows
    .slice(first)
    .filter((row) => row.getBoundingClientRect().top < viewBottom - 2)
    .map((row) => ({ ids: idsOf(row), offset: Math.round(row.getBoundingClientRect().top - viewTop) }))
    .filter((row) => row.ids.length > 0)
}

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
  // Where the shown rows start in the loaded list; null is the newest PAGE rows. The index alone cannot follow a reader
  // through a row above it being removed or several rows regrouping into one, so the pin also carries the host entries
  // the shown rows hold. The first that survives is the reader's first row wherever the rebuilt list put it.
  const [firstShown, setFirstShown] = useState<WindowStart>(null)
  const [anchors, setAnchors] = useState<string[]>([])
  const [loadingEarlier, setLoadingEarlier] = useState(false)
  const firstShownRef = useRef<WindowStart>(null)
  const start = pinnedWindowStart(all, firstShown, anchors, PAGE)
  const items = all.slice(start)
  const more = start > 0 || canLoadOlder
  const loadingEarlierRef = useRef(false)
  // A new request/navigation generation. A history response from an older generation cannot move a reader who has
  // since jumped to the newest or been sent to another place.
  const pageRequest = useRef(0)
  const showWindow = (from: WindowStart, rows: ChatItem[] = all) => {
    firstShownRef.current = from
    setFirstShown(from)
    setAnchors(windowAnchors(rows, from, PAGE))
  }
  // Following the newest message, or reading history: scrolling up releases the bottom,
  // arriving back at the end (or Jump to latest) restores it. Content growth alone never changes it.
  const [following, setFollowingState] = useState(true)
  // Set with the state, not from it: a store change renders before a pending change to it, and would undo it.
  const followingRef = useRef(true)
  const setFollowing = (on: boolean) => {
    const was = followingRef.current
    followingRef.current = on
    setFollowingState(on)
    // Coming back from history, the window is the newest page again: keeping an older start would let arrivals grow
    // the shown rows without bound. A follower already at the newest keeps a page it opened with Load earlier.
    if (on && !was) {
      pageRequest.current += 1
      showWindow(null)
    }
  }
  const scroller = useRef<HTMLDivElement>(null)
  const lastScrollTop = useRef(0)
  const keepPlace = useRef<Place | null>(null)
  // The page a place was taken for, until that page has committed. The place is armed before the fetch resolves, so
  // the renders on the way — the "loading" one, a store change, a streaming reply — must not be read as "the hold
  // is done" and dropped, or the arrival that follows would go unpaid for.
  const placePending = useRef(false)
  // The view moving for the place, as opposed to the reader moving it. A shift can land exactly at the end — the
  // browser clamps it there — and that must not read as the reader arriving at the newest and asking to follow the
  // chat again: they are in the history on purpose. Holds the scrollTop the shift left the view at, and the scroll
  // event that value raises consumes it; a shift the browser clamped away raises none and arms nothing.
  const ownScroll = useRef<number | null>(null)
  // The rows crossing the view while the reader is in history. A row removed above the viewport shifts the row being
  // read even though the window start and its anchors still name the right row; this keeps that row pixel-still. It is
  // refreshed on the reader's own scroll and after every render, so it describes what is actually in front of them.
  const readingRows = useRef<ReadingRow[]>([])
  // The scrollTop those offsets were measured at. The reader can move the view before its scroll event is delivered;
  // comparing DOM offsets taken at two scroll positions would then read that move as a list mutation and undo it.
  const readingScrollTop = useRef(0)
  const rememberReading = (el: HTMLElement) => {
    if (followingRef.current) return
    readingRows.current = readingRowsOf(el)
    readingScrollTop.current = el.scrollTop
  }
  /**
   * Pays for a list mutation using the rows in front of the reader. The saved offsets describe the scroll position
   * when they were taken, so first account for how far the reader has moved since then. A mutation below the view then
   * has a zero shift; one above it is corrected without discarding a scroll the reader just made.
   */
  const compensateReading = (el: HTMLElement) => {
    const movedSince = el.scrollTop - readingScrollTop.current
    for (const was of readingRows.current) {
      const row = rowAmong(el, was.ids)
      if (!row) continue
      const now = Math.round(row.getBoundingClientRect().top - el.getBoundingClientRect().top)
      const moved = now - (was.offset - movedSince)
      if (Math.abs(moved) >= 1) {
        const before = el.scrollTop
        el.scrollTop += moved
        const after = el.scrollTop
        if (after !== before) ownScroll.current = after
        lastScrollTop.current = after
      }
      return
    }
  }
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
    pageRequest.current += 1
    // The reveal moves the view itself below; the pixel hold from before would pull it back to the old place.
    readingRows.current = []
    showWindow(revealWindowStart(list, revealed.entryId, firstShownRef.current, PAGE, followingRef.current), list)
    setRevealing({ entryId: revealed.entryId })
  }, [revealed])
  // At once, not smooth: a smooth scroll still at the end lets the follower take the chat back down. Brought up from
  // the newest, the reader stops following it.
  useEffect(() => {
    const el = scroller.current
    if (!el || !revealing) return
    rowShowing(el, revealing.entryId)?.scrollIntoView({ block: 'center' })
    if (el.scrollHeight - el.scrollTop - el.clientHeight >= 8) setFollowing(false)
  }, [revealing])

  const showEarlier = async () => {
    if (loadingEarlierRef.current) return
    loadingEarlierRef.current = true
    setLoadingEarlier(true)
    const request = ++pageRequest.current
    const el = scroller.current
    // The row the reader is on, kept for however many rows go in above it. A reader at the newest keeps none: the
    // rows are brought to the bottom after every render there, and a place would only fight that.
    keepPlace.current = el ? keptPlace(placesOf(el), followingRef.current) : null
    // Armed with it: until the page this request brings has committed, a render that finds nothing to move has not
    // finished the hold, and dropping it there would leave the arrival unpaid for.
    placePending.current = keepPlace.current !== null
    // The entry the reader's first row is about, which is what the rebuilt list is asked for. Not the first row's
    // own id: that is often a date separator, and the separator stops existing as soon as rows above it arrive.
    const wasShowing = entryFrom(all, start)
    try {
      // A page already held above the window opens locally. At the start of the loaded list the computer is asked,
      // including when the only cached row above the reader is a date separator: that press must not stop there and
      // make older messages take a second one.
      let list = fresh()
      let plan = pagePlan(list, wasShowing, PAGE, canLoadOlder)
      if (plan.fetch) {
        await loadOlder()
        // Read after the wait: the rows that went in are above everything held, so the row the reader is on sits that
        // many rows further down. With nothing added, the rows themselves are already held and one page is all there is.
        list = fresh()
        plan = pagePlan(list, wasShowing, PAGE, canLoadOlder)
      }
      // An answer from before Jump to latest, a reveal or another navigation did not move the reader then, and must
      // not move them now. The reader's own entry no longer being in the list also leaves the window where it was:
      // moving it would show a page of the conversation the reader was not reading.
      if (request === pageRequest.current && plan.start !== null) showWindow(plan.start, list)
    } finally {
      // The request is over: the page it brought is in this list, and the render that shows it is the last one the
      // place is owed for. Cleared after the sets, never before, so every render up to that one still holds it.
      placePending.current = false
      loadingEarlierRef.current = false
      setLoadingEarlier(false)
    }
  }

  // Rows going in above keep the reader's place; a follower stays at the bottom. This runs after every render, so the
  // place has to survive whatever renders come first: it moves the view only by how far the row itself moved in the
  // content, which is zero for a message arriving below, for the box growing, and for the fetch's own "loading"
  // render — and it is dropped once the page it was taken for has committed and paid for.
  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    const place = keepPlace.current
    // No page action is holding a place: keep the row crossing the view pixel-still through any other list mutation.
    // This is what pays for deleting a row above the viewport (or several rows regrouping) while the window start
    // itself still resolves to a surviving anchor.
    if (!followingRef.current && !place && readingRows.current.length > 0) {
      compensateReading(el)
    }
    if (place && !followingRef.current) {
      // A place taken on an entry that has since merged into a group belongs to that group's row.
      const row = rowShowing(el, place.id)
      if (!row) {
        // The row the page was taken on is gone. Before recording the already-shifted view, try the other rows that
        // were crossing it, in this same layout pass: deleting the active paging row must not lose the next survivor.
        compensateReading(el)
        keepPlace.current = null
      } else {
        const moved = shiftTo(place, contentTopOf(el, row))
        if (moved !== 0) {
          const was = el.scrollTop
          el.scrollTop += moved
          const now = el.scrollTop
          // Only a scroll that actually moved the view raises the event to swallow. A shift the browser clamped away
          // changes nothing and raises nothing, so nothing is armed for the reader's own next scroll to be lost in.
          if (now !== was) ownScroll.current = now
          lastScrollTop.current = now
          // The difference is paid once, and the place becomes where the row is NOW. A difference is never closed by
          // moving the view — the row's start in the content is the same number however it is scrolled — so a place
          // left as it was taken reads the same difference next render and pays it again: 0, 2120, 4240, 6360, 8480,
          // 10600, five renders of a streaming reply that never added a row. Re-anchoring also accepts a view the
          // browser clamped short of the distance asked for: the row did not reach where it should, so the hold
          // becomes where it actually got to rather than the rest of a debt.
          keepPlace.current = settledPlace(place, contentTopOf(el, row))
        } else if (!loadingEarlier && !placePending.current) {
          // The request has committed its page and there is nothing left to move: the reader's place is theirs again.
          keepPlace.current = null
        }
      }
    } else if (followingRef.current) {
      // Cleared here too: a place that outlived the render which took it would move a reader who has since left
      // the bottom, and the branch above would never see it again.
      keepPlace.current = null
      placePending.current = false
      toBottom()
    }
    rememberReading(el)
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
          // The place's own scroll, not the reader's: a clamp can land exactly at the end, and turning following on
          // there would take the reader's place from them with nobody having asked for it. Consumed at most once —
          // a scroll that is not the one the shift left raises is the reader's and is handled as one.
          const mine = ownScroll.current
          if (mine !== null) {
            ownScroll.current = null
            if (Math.abs(el.scrollTop - mine) < 2) {
              lastScrollTop.current = el.scrollTop
              rememberReading(el)
              return
            }
          }
          const atEnd = el.scrollHeight - el.scrollTop - el.clientHeight < 8
          if (el.scrollTop < lastScrollTop.current - 1 && !atEnd) {
            // Pin the window where it is before an arrival can remove rows above this reader. Without a page taken
            // yet the start is still null (the newest page), and each incoming row would otherwise move it down and
            // drop a row the reader is looking at. The pin carries the entries shown from there so a deletion or a
            // grouped approval above them cannot make the same index name a different row.
            if (firstShownRef.current === null) showWindow(start)
            setFollowing(false)
          } else if (atEnd) setFollowing(true)
          lastScrollTop.current = el.scrollTop
          rememberReading(el)
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
            <div key={item.id} data-row-id={item.id} className={arrivals.has(item.id) ? rowIn(item) : undefined} {...rowIdentity(item)}>
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
