import { useState } from 'react'
import { isGroup } from '@shared/models'
import { taskLine, threadSections, type WorkTask } from '@shared/work'
import { BotAvatar } from '../components/Avatar'
import { IconButton } from '../components/Controls'
import { Icon } from '../components/Icon'
import { HStack } from '../components/Stack'
import { Text } from '../components/Text'
import { prefs, usePref } from '../lib/prefs'
import { useTr } from '../lib/use-language'
import { useScreenViewed } from '../store/analytics'
import { refKey, sameRef, type BotRef, type RosterItem } from '../store/app-model'
import { StoreContext } from '../store/context'
import { relativeTime } from './chat/chat-items'
import { WORK_MARK, WorkDot } from './chat/WorkDialogs'
import { ThreadChat } from './chat/ThreadChat'
import { ThreadStart } from './chat/ThreadStart'
import './computer-roster.css'
import './threads-page.css'

/** The thread the Threads page shows: a task of a bot's. */
export interface ThreadPick {
  ref: BotRef
  taskId: string
}

const SHOWN_RECENT = 8

/**
 * The Threads page's list, in the shape of a chat app's sidebar: **New thread** first, then what needs the user, then
 * the groups the list's options make (`threadsGroup`: the folders threads work in outside their bot's own and the
 * routines' runs, a group per bot in the bot list's order, or one per computer), then the rest, newest first, the last of it on demand.
 * A row is a thread's title and one mark at its end for how it stands; a group's heading folds it, and a bot's has a
 * `+` that starts a thread with it (`start`). A row of a task a thread has or had shows its thread beside the list
 * (`pick`). What a task is told to do (first, cut in, drop, try again) stays on its bot's Details.
 */
export function ThreadList({ roster, picked, pick, start }: {
  roster: RosterItem[]
  picked: ThreadPick | null
  pick: (thread: ThreadPick) => void
  start: (ref: BotRef | null) => void
}) {
  const tr = useTr()
  useScreenViewed('all_work')
  const [grouping] = usePref(prefs.threadsGroup)
  const [folded, setFolded] = usePref(prefs.foldedThreadSections)
  const [more, setMore] = useState(false)
  const { needsYou, groups, recent } = threadSections(roster, grouping, (of) => refKey(of.ref), (of) => of.store.hostName)
  const canStart = threadBots(roster).length > 0
  const row = ({ of, task }: { of: RosterItem; task: WorkTask }, label = task.title) => (
    <StoreContext.Provider key={`${refKey(of.ref)}/${task.id}`} value={of.store}>
      <ThreadRow
        task={task}
        label={label}
        selected={picked !== null && picked.taskId === task.id && sameRef(picked.ref, of.ref)}
        open={task.thread ? () => pick({ ref: of.ref, taskId: task.id }) : undefined}
      />
    </StoreContext.Provider>
  )
  const label = (title: string) => <Text as="div" size="caption" tone="tertiary" className="thread-label">{tr(title)}</Text>
  const shownRecent = more ? recent : recent.slice(0, SHOWN_RECENT)
  return (
    <>
      <button className="thread-row thread-new" disabled={!canStart} onClick={() => start(null)}>
        <Icon name="square.and.pencil" size={13} color="var(--secondary)" />
        <Text tone="text" truncate>{tr('New thread')}</Text>
      </button>
      {needsYou.length ? <section aria-label={tr('Needs you')}>{label('Needs you')}{needsYou.map((r) => row(r))}</section> : null}
      {(['folder', 'routine', 'bot', 'computer'] as const).map((kind) => {
        const mine = groups.filter((g) => g.kind === kind)
        if (!mine.length) return null
        const title = kind === 'folder' ? 'Folders' : kind === 'routine' ? 'Routines' : kind === 'bot' ? 'Bots' : 'Computers'
        return (
          <section key={kind} aria-label={tr(title)}>
            {label(title)}
            {mine.map((group) => {
              const isFolded = folded.includes(group.key)
              const of = group.rows[0]!.of
              // Folded, its heading still says that something in it went wrong or works.
              const loud = isFolded ? (group.rows.find((r) => r.task.status === 'failed') ?? group.rows.find((r) => r.task.status === 'working')) : undefined
              return (
                <div key={group.key} className="thread-group" role="group" aria-label={group.title}>
                  <HStack gap={2} className="thread-section-heading">
                    <button className="thread-row thread-section-toggle" aria-expanded={!isFolded} onClick={() => setFolded(isFolded ? folded.filter((key) => key !== group.key) : [...folded, group.key])}>
                      {group.kind === 'bot' ? <BotAvatar bot={of.bot} size={16} animated={false} /> : <Icon name={({ folder: 'folder', routine: 'clock', computer: 'desktopcomputer' } as const)[group.kind]} size={12} color="var(--secondary)" />}
                      <Text tone="text" truncate grow>{group.title}</Text>
                      {loud ? <StatusMark status={loud.task.status} /> : null}
                    </button>
                    {group.kind === 'bot' && threadBots([of]).length ? <IconButton title="New thread" icon="plus" size={24} className="thread-section-new" onClick={() => start(of.ref)} /> : null}
                  </HStack>
                  <div className={`computer-roster-content ${isFolded ? 'collapsed' : ''}`} inert={isFolded} aria-hidden={isFolded}>
                    {/* A routine's runs are told apart by when they started. */}
                    <div className="thread-group-rows">{group.rows.map((r) => row(r, group.kind === 'routine' ? relativeTime.separator(r.task.createdAt, tr) : r.task.title))}</div>
                  </div>
                </div>
              )
            })}
          </section>
        )
      })}
      {recent.length ? (
        <section aria-label={tr('Recent')}>
          {label('Recent')}
          {shownRecent.map((r) => row(r))}
          {recent.length > shownRecent.length ? (
            <button className="thread-row thread-more" onClick={() => setMore(true)}>
              <Text tone="tertiary">{tr('Show more')}</Text>
            </button>
          ) : null}
        </section>
      ) : null}
    </>
  )
}

/** How a thread stands, at the end of its row: working, needs the user, went wrong, waits; nothing once it is done. */
function StatusMark({ status }: { status: WorkTask['status'] }) {
  return status === 'done' ? null : <WorkDot mark={WORK_MARK[status]} />
}

/** One thread of the list: its title (a routine's run: when it started) and its mark. */
function ThreadRow({ task, label, selected, open }: { task: WorkTask; label: string; selected: boolean; open?: () => void }) {
  const Open = open ? 'button' : 'div'
  return (
    <Open className={`thread-row ${selected ? 'selected' : ''}`} data-thread-row={task.title} data-work-status={task.status} onClick={open} title={task.title}>
      <Text tone="text" truncate grow>{label}</Text>
      <StatusMark status={task.status} />
    </Open>
  )
}

/** The bots a thread can start with: on a computer that is connected and starts threads; never a group. */
export const threadBots = (roster: RosterItem[]) => roster.filter((r) => !isGroup(r.bot) && r.store.startsThreads && !r.store.isOffline)

/**
 * Beside the list: the thread picked there, its title, status, transcript and message box (`WorkThread`); one that
 * still waits for a place says so. `showInChat` brings an entry of its bot's chat into view there. With none picked,
 * or one whose task left its bot's box, the page a new thread starts from (`NewThread`, with the bot `starting`
 * names, else the lead bot), or the page's mark where no bot can start one.
 */
export function ThreadDetail({ roster, picked, pick, starting, start }: {
  roster: RosterItem[]
  picked: ThreadPick | null
  pick: (thread: ThreadPick) => void
  starting: BotRef | null
  start: (ref: BotRef) => void
}) {
  const tr = useTr()
  const of = picked ? roster.find((r) => sameRef(r.ref, picked.ref)) : undefined
  const task = picked ? of?.bot.work.find((i) => i.id === picked.taskId) : undefined
  if (of && task?.thread) {
    return (
      <StoreContext.Provider value={of.store}>
        <ThreadChat key={`${refKey(of.ref)}/${task.id}`} bot={of.bot} task={task} />
      </StoreContext.Provider>
    )
  }
  if (task) {
    return (
      <div className="empty-detail">
        <div className="detail-drag drag" />
        <Text as="div" size="headline" tone="text" className="empty-detail-message">{task.title}</Text>
        <Text as="div" size="callout" tone="secondary">{taskLine(task, tr)}</Text>
      </div>
    )
  }
  const bots = threadBots(roster)
  const first = bots.find((r) => sameRef(r.ref, starting)) ?? bots.find((r) => r.bot.lead) ?? bots[0]
  if (!first) {
    return (
      <div className="empty-detail">
        <div className="detail-drag drag" />
        <Icon name="checklist" size={34} color="var(--tertiary)" />
      </div>
    )
  }
  return (
    <StoreContext.Provider value={first.store}>
      <ThreadStart key={refKey(first.ref)} bots={bots} of={first} choose={start} started={(taskId) => pick({ ref: first.ref, taskId })} />
    </StoreContext.Provider>
  )
}
