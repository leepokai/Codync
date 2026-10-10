import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { isWorkingIn } from '@shared/models'
import { IconButton } from '../../components/Controls'
import { Sheet } from '../../components/Overlay'
import { font } from '../../lib/fonts'
import { useStore } from '../../store/context'
import { UpdateNeededCard } from '../UpdateNeededCard'
import { buildChat, rowIn, useArrivals } from './chat-items'
import { ChatRow, WorkingIndicator } from './ChatRows'
import { Composer } from './Composer'
import { TraceView } from './TraceView'
import { useReading } from './reading'

/**
 * A thread (Slack's "reply in thread"): the message it started on, its replies and a box to
 * continue there. In a bot's own chat the thread is a separate branch of the conversation (its
 * own session, forked from the chat); in a group the room answers inside the thread.
 */
export function RepliesView({ botId, rootId, close }: { botId: string; rootId: string; close: () => void }) {
  const store = useStore()
  const chat = store.bots.get(botId) ?? null
  const root = store.allEntries(botId).find((e) => e.id === rootId)
  const replies = store.replies(botId, rootId)
  const live = !!chat && isWorkingIn(chat, botId, rootId) && !store.isOffline
  const items = buildChat(replies)
  const arrivals = useArrivals([...items.map((i) => i.id), ...(live ? ['working'] : [])], true)
  const [showTrace, setShowTrace] = useState(false)
  const scroller = useRef<HTMLDivElement>(null)
  useReading(store, botId, rootId)

  useEffect(() => {
    void store.loadThread(botId, rootId)
  }, [store, botId, rootId])

  useLayoutEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [replies.length, replies[replies.length - 1]?.data.text, live])

  const openTrace = () => setShowTrace(true)
  return (
    <div style={{ display: 'flex', flexDirection: 'column', height: '100%', background: 'var(--background)' }}>
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, height: 48, padding: '0 10px 0 16px', flex: 'none' }}>
        <div style={{ display: 'flex', flexDirection: 'column', flex: 1, minWidth: 0 }}>
          <span style={{ ...font('compactBody', 'semibold'), color: 'var(--text)' }}>Thread</span>
          <span style={{ ...font('caption'), color: 'var(--tertiary)', whiteSpace: 'nowrap', overflow: 'hidden', textOverflow: 'ellipsis' }}>{chat?.name ?? ''}</span>
        </div>
        <IconButton title="Close thread" icon="xmark" onClick={close} />
      </div>
      <div style={{ height: 0.5, background: 'var(--border)', flex: 'none' }} />
      <div ref={scroller} style={{ flex: 1, overflowY: 'auto' }}>
        <div style={{ padding: '8px 16px 0' }}>
          {root ? (
            <>
              <ChatRow entry={root} groupStart chat={chat} openTrace={openTrace} />
              <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '12px 0' }}>
                <span style={{ ...font('caption'), color: 'var(--tertiary)', whiteSpace: 'nowrap' }}>
                  {replyCount(root.data.thread?.count ?? 0)}
                </span>
                <div style={{ flex: 1, height: 1, background: 'var(--border)' }} />
              </div>
            </>
          ) : null}
          {items.map((item) =>
            item.kind === 'entry' ? (
              <div key={item.id} className={arrivals.has(item.id) ? rowIn(item) : undefined}>
                <ChatRow entry={item.entry} groupStart={item.groupStart} chat={chat} openTrace={openTrace} />
              </div>
            ) : null,
          )}
          {chat && live ? (
            <div className={arrivals.has('working') ? 'row-in' : undefined} style={{ paddingTop: 6 }}>
              <WorkingIndicator bot={chat} thinking={store.currentThinking(botId, rootId)} />
            </div>
          ) : null}
          <div style={{ height: 8 }} />
        </div>
      </div>
      <div style={{ flex: 'none' }}>
        {store.mismatch ? (
          <div style={{ padding: '0 16px 8px' }}>
            <UpdateNeededCard />
          </div>
        ) : (
          <Composer botId={botId} thread={rootId} />
        )}
      </div>
      <Sheet open={showTrace} onClose={() => setShowTrace(false)} width={620} height={560}>
        <TraceView botId={botId} thread={rootId} />
      </Sheet>
    </div>
  )
}

/** The host's count, as on the thread's chip: messages, not the agent's trace. */
function replyCount(count: number) {
  return count === 0 ? 'No replies yet' : count === 1 ? '1 reply' : `${count} replies`
}
