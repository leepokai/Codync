import { useState } from 'react'
import { allThreads, itemLine, pendingAsks, type WorkItem } from '@shared/work'
import { Icon } from '../components/Icon'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import { prefs, usePref } from '../lib/prefs'
import { useTr } from '../lib/use-language'
import { useScreenViewed } from '../store/analytics'
import { refKey, sameRef, type BotRef, type RosterItem } from '../store/app-model'
import { StoreContext } from '../store/context'
import { ArtifactView } from './library/ArtifactView'
import { Timer } from './thread/WorkingIndicator'
import { WorkRow } from './thread/WorkRows'
import { WorkThread } from './thread/WorkThread'
import './threads-page.css'

/** The thread the Threads page shows: an item of a bot's. */
export interface ThreadPick {
  ref: BotRef
  itemId: string
}

/**
 * The Threads page's list: every bot's work, by status, under its computer's heading, both or neither as the list's
 * options say, each row an item with its bot under its title (and the bot's computer with several, `many`, unless
 * its heading says it). A row of an item a task has or had shows its thread beside the list (`pick`); one
 * that needs the user opens its bot's chat at the card it waits on (`openCard`). What an item is told to do (first,
 * cut in, drop, try again) stays on its bot's Details.
 */
export function ThreadList({ roster, many, picked, pick, openCard }: {
  roster: RosterItem[]
  many: boolean
  picked: ThreadPick | null
  pick: (thread: ThreadPick) => void
  openCard: (ref: BotRef, itemId: string) => void
}) {
  const tr = useTr()
  useScreenViewed('all_work')
  const [byComputer] = usePref(prefs.threadsByComputer)
  const [byStatus] = usePref(prefs.threadsByStatus)
  const { groups } = allThreads(roster, byStatus, byComputer ? (of) => of.store.hostName : undefined)
  if (!groups.length) {
    return (
      <div className="roster-empty">
        <Text as="div" size="callout" tone="secondary">{tr('Nothing in the background')}</Text>
      </div>
    )
  }
  return (
    <>
      {groups.map((group) => (
        <section key={group.title} className="thread-group" aria-label={group.named ? group.title : tr(group.title || 'Threads')}>
          {group.title ? (
            <HStack gap={6} align="baseline" className="thread-group-heading">
              <Text size="caption" weight="semibold" tone="secondary" truncate>{group.named ? group.title : tr(group.title)}</Text>
              <Text size="caption" tone="tertiary" tabular>{group.rows.length}</Text>
            </HStack>
          ) : null}
          {group.rows.map(({ of, item }) => (
            <StoreContext.Provider key={`${refKey(of.ref)}/${item.id}`} value={of.store}>
              <ThreadRow
                of={of}
                item={item}
                many={many && !byComputer}
                selected={picked !== null && picked.itemId === item.id && sameRef(picked.ref, of.ref)}
                open={item.status === 'needsYou' ? () => openCard(of.ref, item.id) : item.task ? () => pick({ ref: of.ref, itemId: item.id }) : undefined}
              />
            </StoreContext.Provider>
          ))}
        </section>
      ))}
    </>
  )
}

function ThreadRow({ of, item, many, selected, open }: { of: RosterItem; item: WorkItem; many: boolean; selected: boolean; open?: () => void }) {
  const tr = useTr()
  const says = item.status === 'needsYou' ? pendingAsks(of.store.allEntries(of.bot.id), tr).get(item.id) || itemLine(item, tr) : itemLine(item, tr)
  const by = many ? `${of.bot.name} · ${of.store.hostName}` : of.bot.name
  return (
    <div className={`thread-row ${selected ? 'selected' : ''}`} data-thread-row={item.title}>
      <WorkRow item={item} line={`${by} · ${says}`} open={open} time={item.status === 'working' && item.startedAt !== null ? <Timer since={item.startedAt} /> : null} />
    </div>
  )
}

/**
 * The thread picked in the list: its title, status, transcript and message box (`WorkThread`). `showInChat` brings an
 * entry of its bot's chat into view there. With none picked, or one whose item left its bot's box, the page's mark.
 */
export function ThreadDetail({ roster, picked, showInChat }: { roster: RosterItem[]; picked: ThreadPick | null; showInChat: (ref: BotRef, entryId: string) => void }) {
  const of = picked ? roster.find((r) => sameRef(r.ref, picked.ref)) : undefined
  const item = picked ? of?.bot.work.find((i) => i.id === picked.itemId && i.task) : undefined
  if (!of || !item) {
    return (
      <div className="empty-detail">
        <div className="detail-drag drag" />
        <Icon name="tray.full" size={34} color="var(--tertiary)" />
      </div>
    )
  }
  return (
    <StoreContext.Provider value={of.store}>
      <OpenThread key={`${refKey(of.ref)}/${item.id}`} of={of} item={item} showInChat={(entryId) => showInChat(of.ref, entryId)} />
    </StoreContext.Provider>
  )
}

function OpenThread({ of, item, showInChat }: { of: RosterItem; item: WorkItem; showInChat: (entryId: string) => void }) {
  // The artifact shown over the thread (a card of its transcript opened it).
  const [artifactId, setArtifactId] = useState<string | null>(null)
  return (
    <div className="thread-detail">
      <WorkThread botId={of.bot.id} item={item} openArtifact={setArtifactId} showInChat={showInChat} />
      {artifactId ? <ArtifactView botId={of.bot.id} artifactId={artifactId} close={() => setArtifactId(null)} /> : null}
    </div>
  )
}
