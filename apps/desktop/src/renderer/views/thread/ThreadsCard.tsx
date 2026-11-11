import { useEffect, useMemo, useState } from 'react'
import { threadName, type Bot, type ThreadInfo } from '@shared/models'
import { CardSection, DropdownMenu, IconButton, TabChoice } from '../../components/Controls'
import { Dialog, Sheet } from '../../components/Overlay'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useModel } from '../../lib/observable'
import { useTr } from '../../lib/use-language'
import type { BotStore } from '../../store/bot-store'
import { relativeTime } from './chat-items'
import { RepliesView } from './RepliesView'

type Section = 'open' | 'archived'

const SECTIONS: { id: Section; label: string }[] = [
  { id: 'open', label: 'Open' },
  { id: 'archived', label: 'Archived' },
]

/**
 * A chat's threads (`OpenBot` Mobile's): side conversations on one message, the open ones or the
 * archive, newest first with the last line of each. Opening one puts it over the chat.
 */
export function ThreadsCard({ bot, store }: { bot: Bot; store: BotStore }) {
  const tr = useTr()
  useModel(store)
  const [section, setSection] = useState<Section>('open')
  const archived = section === 'archived'
  const threads = (archived ? store.archivedThreads : store.threads).get(bot.id) ?? []
  const [open, setOpen] = useState<string | null>(null)
  const [renaming, setRenaming] = useState<ThreadInfo | null>(null)
  const [renameText, setRenameText] = useState('')
  const [deleting, setDeleting] = useState<ThreadInfo | null>(null)
  useEffect(() => {
    void store.refreshThreads(bot.id, archived)
  }, [store, bot.id, archived])
  const saveRename = () => {
    if (!renaming) return
    void store.renameThread(bot.id, renaming.id, renameText)
  }
  const rowMenu = useMemo(() => (thread: ThreadInfo) => () => [
    { title: 'Rename thread', icon: 'pencil', action: () => { setRenameText(thread.title); setRenaming(thread) } },
    thread.archived
      ? { title: 'Restore', icon: 'tray.and.arrow.up', action: () => void store.setThreadArchived(bot.id, thread.id, false) }
      : { title: 'Archive thread', icon: 'archivebox', action: () => void store.setThreadArchived(bot.id, thread.id, true) },
    { title: 'Delete thread', icon: 'trash', destructive: true, divider: true, action: () => setDeleting(thread) },
  ], [bot.id, store])
  return (
    <div className="threads-panel">
      <HStack gap={8} align="center">
        <span className="threads-title">{tr('Threads')}</span>
        <IconButton title="New thread" icon="plus" disabled={store.isOffline} onClick={() => void store.startThread(bot.id).then(setOpen)} />
      </HStack>
      <TabChoice label={tr('Threads')} selection={section} options={SECTIONS} onChange={setSection} />
      <CardSection>
        {!threads.length ? (
          <Text as="div" tone="secondary">
            {archived ? 'Nothing archived.' : 'No threads yet. Open one from the menu, or branch from a message.'}
          </Text>
        ) : null}
        {threads.map((thread) => (
          <div key={thread.id} className="threads-row">
            <button className="threads-open press" onClick={() => setOpen(thread.id)}>
              <VStack gap={2}>
                <HStack gap={6} align="center">
                  {thread.unread ? <span className="thread-dot" aria-hidden /> : null}
                  <Text weight="semibold" lines={1}>{tr(threadName(thread))}</Text>
                </HStack>
                <Text size="caption" tone="secondary" lines={1}>
                  {thread.lastText
                    ? `${thread.lastAuthor === 'user' ? tr('You') : (thread.lastAuthor ?? '')} · ${relativeTime.day(thread.updatedAt)}`
                    : relativeTime.day(thread.updatedAt)}
                </Text>
              </VStack>
            </button>
            <DropdownMenu title="Thread actions" className="threads-menu" items={rowMenu(thread)}><span className="threads-menu-ellipsis">⋯</span></DropdownMenu>
          </div>
        ))}
      </CardSection>
      <Dialog open={renaming !== null} title="Rename thread" actions={[{ title: 'Save', action: saveRename }]} onClose={() => setRenaming(null)}>
        <input className="dialog-field" data-autofocus value={renameText} placeholder={tr('A short name')} aria-label={tr('A short name')} spellCheck={false}
          onChange={(e) => setRenameText(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== 'Enter' || e.nativeEvent.isComposing || e.keyCode === 229) return
            e.preventDefault()
            setRenaming(null)
            saveRename()
          }} />
      </Dialog>
      <Dialog open={deleting !== null} title="Delete thread?" message="Its replies go with it; the chat stays."
        actions={[{
          title: 'Delete thread',
          destructive: true,
          action: () => {
            if (deleting) void store.deleteThread(bot.id, deleting.id)
            if (deleting?.id === open) setOpen(null)
          },
        }]}
        onClose={() => setDeleting(null)} />
      <Sheet open={open !== null} onClose={() => setOpen(null)} width={620} height={560}>
        {open ? <RepliesView botId={bot.id} rootId={open} close={() => setOpen(null)} /> : null}
      </Sheet>
    </div>
  )
}
