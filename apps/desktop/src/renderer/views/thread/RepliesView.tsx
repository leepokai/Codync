import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { isWorkingIn } from '@shared/models'
import { Button } from '../../components/Controls'
import { ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { UpdateNeededCard } from '../UpdateNeededCard'
import { buildChat, rowIn, useArrivals } from './chat-items'
import { ChatRow } from './ChatRows'
import { WorkingIndicator } from './WorkingIndicator'
import { workingSteps } from './trace-turns'
import { Composer } from './Composer'
import { TraceView } from './TraceView'

/**
 * A document's own conversation (`doc:<id>`): the bot answers about that page in a session of its
 * own, kept out of the chat (the chat itself has no threads; a message there replies to another).
 */
export function RepliesView({ botId, rootId, close, documentTitle }: { botId: string; rootId: string; close: () => void; documentTitle?: string }) {
  const store = useStore()
  const chat = store.bots.get(botId) ?? null
  const replies = store.allEntries(botId).filter((e) => e.threadId === rootId)
  const live = !!chat && isWorkingIn(chat, botId) && !store.isOffline
  const items = buildChat(replies)
  const arrivals = useArrivals([...items.map((i) => i.id), ...(live ? ['working'] : [])], true)
  const [showTrace, setShowTrace] = useState(false)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [retry, setRetry] = useState(0)
  const scroller = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let active = true
    setLoading(true); setLoadError(null)
    void store.loadThread(botId, rootId).catch((error: unknown) => {
      if (active) setLoadError(error instanceof Error ? error.message : String(error))
    }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [store, botId, rootId, store.connection.kind, retry])

  useLayoutEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [replies.length, replies[replies.length - 1]?.data.text, live])

  const openTrace = () => setShowTrace(true)
  return (
    <SheetPage>
      <ModalHeader title="Document conversation" subtitle={[documentTitle, chat?.name].filter(Boolean).join(' · ')} onClose={close}
        closeTitle="Close document conversation" />
      <div className="thread-hairline" />
      <div ref={scroller} className="thread-scroll">
        <div className="replies-content">
          {loading && <Text as="div" tone="secondary" role="status">Loading conversation…</Text>}
          {loadError && <div role="alert"><Text as="div" tone="danger">{loadError}</Text><Button kind="secondary" onClick={() => setRetry((n) => n + 1)}>Retry loading conversation</Button></div>}
          {!loading && !loadError && !items.length && !live && <Text as="div" tone="secondary">Ask this bot about the saved document. This page has its own conversation.</Text>}
          {items.map((item) =>
            item.kind === 'entry' ? (
              <div key={item.id} className={arrivals.has(item.id) ? rowIn(item) : undefined}>
                <ChatRow entry={item.entry} groupStart={item.groupStart} chat={chat} openTrace={openTrace} />
              </div>
            ) : null,
          )}
          {chat && live ? (
            <div className={arrivals.has('working') ? 'chat-working row-in' : 'chat-working'}>
              <WorkingIndicator bot={chat} steps={workingSteps(replies)} />
            </div>
          ) : null}
          <div className="chat-end" />
        </div>
      </div>
      <div className="replies-composer">
        {store.mismatch ? (
          <div className="composer-update-needed">
            <UpdateNeededCard />
          </div>
        ) : (
          <Composer botId={botId} thread={rootId} />
        )}
      </div>
      <Sheet open={showTrace} onClose={() => setShowTrace(false)} width={620} height={560}>
        <TraceView botId={botId} thread={rootId} />
      </Sheet>
    </SheetPage>
  )
}
