import { useEffect, useLayoutEffect, useRef, useState, type ReactNode, type RefObject } from 'react'
import { Spinner } from '../../components/Controls'
import { usePresence } from '../../components/Overlay'
import { useTr } from '../../lib/use-language'
import { rowIn, useArrivals, type ChatItem } from './chat-items'
import { MacChatButton } from './MacChatButton'

const PAGE = 40

/** Where the shown rows start: at the row `first`, or with the newest `PAGE` rows. */
function startOf(list: ChatItem[], first: string | null) {
  const found = first ? list.findIndex((i) => i.id === first) : -1
  return found >= 0 ? found : Math.max(0, list.length - PAGE)
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
  // The newest 40 items first; older pages come in near the top.
  const [firstShown, setFirstShown] = useState<string | null>(null)
  const [loadingEarlier, setLoadingEarlier] = useState(false)
  const start = startOf(all, firstShown)
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
  const keepPlace = useRef<number | null>(null)
  const last = items[items.length - 1]
  const jump = usePresence(!following && items.length > 0, 200)
  const arrivals = useArrivals(items.map((i) => i.id), following)

  const toBottom = (smooth = false) => {
    const el = scroller.current
    if (el) el.scrollTo({ top: el.scrollHeight, behavior: smooth ? 'smooth' : 'auto' })
  }

  useEffect(() => {
    if (!firstShown && items[0]) setFirstShown(items[0].id)
  }, [firstShown, items])

  // `revealed`: the shown rows reach back to its entry, and it scrolls into the middle.
  const [revealing, setRevealing] = useState<{ entryId: string } | null>(null)
  useEffect(() => {
    if (!revealed) return
    const list = fresh()
    const at = list.findIndex((i) => i.kind !== 'separator' && i.entry.id === revealed.entryId)
    if (at < 0) return
    setFirstShown((first) => list[Math.min(at, startOf(list, first))]!.id)
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
    keepPlace.current = el ? el.scrollHeight - el.scrollTop : null
    let list = all
    let at = start
    if (at === 0) {
      const anchor = list[0]?.id
      await loadOlder()
      list = fresh()
      at = anchor ? Math.max(0, list.findIndex((i) => i.id === anchor)) : 0
    }
    if (at > 0) setFirstShown(list[Math.max(0, at - PAGE)]!.id)
    setLoadingEarlier(false)
  }

  // Earlier messages going in above keep the reader's place; a follower stays at the bottom.
  useLayoutEffect(() => {
    const el = scroller.current
    if (!el) return
    if (keepPlace.current !== null && !followingRef.current) {
      el.scrollTop = el.scrollHeight - keepPlace.current
      keepPlace.current = null
    } else if (followingRef.current) toBottom()
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
          if (more && el.scrollTop < 200 && !loadingEarlier) void showEarlier()
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
