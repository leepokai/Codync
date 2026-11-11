import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { isWorkingIn, threadName, type Entry, type ThreadInfo } from '@shared/models'
import { Button, IconButton } from '../../components/Controls'
import { AnchoredMenu, Dialog, ModalHeader, Sheet, SheetPage } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useTr } from '../../lib/use-language'
import { UpdateNeededCard } from '../UpdateNeededCard'
import { buildChat, rowIn, useArrivals } from './chat-items'
import { ChatRow } from './ChatRows'
import { ForwardSheet } from './ForwardSheet'
import { WorkingIndicator } from './WorkingIndicator'
import { workingSteps } from './trace-turns'
import { Composer } from './Composer'
import { TraceView } from './TraceView'

/**
 * One thread over the chat: the message it branched from, its replies and a box to continue there
 * (`OpenBot` Mobile's thread). A document's side conversation (`doc:<id>`) is one too, with its
 * document's title and no thread actions.
 */
export function RepliesView({ botId, rootId, close, documentTitle }: { botId: string; rootId: string; close: () => void; documentTitle?: string }) {
  const store = useStore()
  const tr = useTr()
  const chat = store.bots.get(botId) ?? null
  const doc = rootId.startsWith('doc:')
  const all = store.allEntries(botId)
  const root = doc ? null : (all.find((e) => e.id === rootId) ?? null)
  const replies = all.filter((e) => e.threadId === rootId)
  const live = !!chat && isWorkingIn(chat, botId) && !store.isOffline
  const items = buildChat(replies)
  const arrivals = useArrivals([...items.map((i) => i.id), ...(live ? ['working'] : [])], true)
  const [showTrace, setShowTrace] = useState(false)
  const [forwardEntry, setForwardEntry] = useState<Entry | null>(null)
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [retry, setRetry] = useState(0)
  const [info, setInfo] = useState<ThreadInfo | null>(null)
  // The replies unread when the thread opened; reading it clears them, so keep the count for this visit.
  const [unread, setUnread] = useState(0)
  const [menuOpen, setMenuOpen] = useState(false)
  const [renaming, setRenaming] = useState(false)
  const [renameText, setRenameText] = useState('')
  const [deleting, setDeleting] = useState(false)
  const menuButton = useRef<HTMLButtonElement>(null)
  const scroller = useRef<HTMLDivElement>(null)

  useEffect(() => {
    let active = true
    setLoading(true); setLoadError(null)
    void store.loadThread(botId, rootId).then((thread) => {
      if (!active) return
      setInfo(thread)
      setUnread(thread?.unread ?? 0)
    }).catch((error: unknown) => {
      if (active) setLoadError(error instanceof Error ? error.message : String(error))
    }).finally(() => { if (active) setLoading(false) })
    return () => { active = false }
  }, [store, botId, rootId, store.connection.kind, retry])

  useLayoutEffect(() => {
    const el = scroller.current
    if (el) el.scrollTop = el.scrollHeight
  }, [replies.length, replies[replies.length - 1]?.data.text, live])

  const openTrace = () => setShowTrace(true)
  const saveRename = () => {
    if (!doc) void store.renameThread(botId, rootId, renameText)
  }
  const title = doc ? tr('Document conversation') : tr(info ? threadName(info) : 'Thread')
  const subtitle = doc ? [documentTitle, chat?.name].filter(Boolean).join(' · ') : (chat?.name ?? '')
  const newLineAfter = Math.max(0, items.length - unread)
  let seen = 0
  return (
    <SheetPage>
      <ModalHeader title={title} subtitle={subtitle} onClose={close}
        closeTitle={doc ? 'Close document conversation' : 'Close thread'}
        trailing={doc ? undefined : (
          <>
            <IconButton title={tr('Thread actions')} icon="ellipsis" buttonRef={menuButton} menuOpen={menuOpen} onClick={() => setMenuOpen((o) => !o)} />
            <AnchoredMenu open={menuOpen} onClose={() => setMenuOpen(false)} anchor={menuButton} items={() => [
              { title: 'Rename thread', icon: 'pencil', action: () => { setRenameText(info?.title ?? ''); setRenaming(true) } },
              info?.archived
                ? { title: 'Restore', icon: 'tray.and.arrow.up', action: () => void store.setThreadArchived(botId, rootId, false) }
                : { title: 'Archive thread', icon: 'archivebox', action: () => void store.setThreadArchived(botId, rootId, true) },
              { title: 'Delete thread', icon: 'trash', destructive: true, divider: true, action: () => setDeleting(true) },
            ]} />
          </>
        )} />
      <div className="thread-hairline" />
      <div ref={scroller} className="thread-scroll">
        <div className="replies-content">
          {loading && <Text as="div" tone="secondary" role="status">Loading conversation…</Text>}
          {loadError && <div role="alert"><Text as="div" tone="danger">{loadError}</Text><Button kind="secondary" onClick={() => setRetry((n) => n + 1)}>Retry loading conversation</Button></div>}
          {!doc && info?.branchText ? <BranchLine text={info.branchText} open={close} /> : null}
          {root ? (
            <>
              <ChatRow entry={root} groupStart={true} chat={chat} openTrace={openTrace} onForward={store.mismatch ? undefined : setForwardEntry} />
              <CountLine count={replies.length} />
            </>
          ) : null}
          {items.map((item) => {
            const line = unread > 0 && item.kind === 'entry' && seen++ === newLineAfter ? <UnreadLine key="new" /> : null
            return item.kind === 'entry' ? (
              <div key={item.id}>
                {line}
                <div className={arrivals.has(item.id) ? rowIn(item) : undefined}>
                  <ChatRow entry={item.entry} groupStart={item.groupStart} chat={chat} openTrace={openTrace} onForward={store.mismatch ? undefined : setForwardEntry} />
                </div>
              </div>
            ) : line
          })}
          {doc && !loading && !loadError && !items.length && !live ? (
            <Text as="div" tone="secondary">Ask this bot about the saved document. This page has its own conversation.</Text>
          ) : null}
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
      <Dialog open={renaming} title="Rename thread" actions={[{ title: 'Save', action: saveRename }]} onClose={() => setRenaming(false)}>
        <input className="dialog-field" data-autofocus value={renameText} placeholder={tr('A short name')} aria-label={tr('A short name')} spellCheck={false}
          onChange={(e) => setRenameText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== 'Enter' || e.nativeEvent.isComposing || e.keyCode === 229) return
            e.preventDefault()
            setRenaming(false)
            saveRename()
          }} />
      </Dialog>
      <Dialog open={deleting} title="Delete thread?" message="Its replies go with it; the chat stays."
        actions={[{ title: 'Delete thread', destructive: true, action: () => { void store.deleteThread(botId, rootId); close() } }]}
        onClose={() => setDeleting(false)} />
      <Sheet open={showTrace} onClose={() => setShowTrace(false)} width={620} height={560}>
        <TraceView botId={botId} thread={rootId} />
      </Sheet>
      <Sheet open={forwardEntry !== null} onClose={() => setForwardEntry(null)} width={420} height={520}>
        {forwardEntry ? <ForwardSheet entry={forwardEntry} close={() => setForwardEntry(null)} /> : null}
      </Sheet>
    </SheetPage>
  )
}

/** The message a thread was branched from, and a jump back to it in the chat. */
function BranchLine({ text, open }: { text: string; open: () => void }) {
  const tr = useTr()
  return (
    <button className="thread-branch press" onClick={open} aria-label={`${tr('Branched from')} ${text}`}>
      <span className="thread-branch-icon" aria-hidden>
        <svg width="12" height="12" viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M3 3v4a2 2 0 0 0 2 2h7" />
          <path d="m9 6 3 3-3 3" />
        </svg>
      </span>
      <span className="thread-branch-copy">
        <Text as="span" size="caption2" weight="medium" tone="tertiary">{tr('Branched from')}</Text>
        <Text as="span" size="caption" tone="secondary" lines={2}>{text}</Text>
      </span>
    </button>
  )
}

/** How many replies a thread has, above them. */
function CountLine({ count }: { count: number }) {
  const label = count === 0 ? 'No replies yet' : count === 1 ? '1 reply' : `${count} replies`
  return (
    <div className="thread-count">
      <Text as="span" size="caption" tone="tertiary">{label}</Text>
      <span className="thread-count-rule" aria-hidden />
    </div>
  )
}

/** Where the unread replies begin. */
function UnreadLine() {
  const tr = useTr()
  return (
    <div className="thread-new">
      <Text as="span" size="caption2" weight="semibold" tone="accent">{tr('New')}</Text>
      <span className="thread-new-rule" aria-hidden />
    </div>
  )
}
