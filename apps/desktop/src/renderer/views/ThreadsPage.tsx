import { useEffect, useRef, useState } from 'react'
import { isGroup } from '@shared/models'
import { agentOf, OWN_FOLDER_GROUP, taskLine, threadSections, type WorkTask } from '@shared/work'
import { AgentIcon } from '../components/AgentIcon'
import { BotAvatar, NoBotAvatar } from '../components/Avatar'
import { IconButton } from '../components/Controls'
import { AnchoredMenu, useContextMenu, type MenuItem } from '../components/Menu'
import { Icon } from '../components/Icon'
import { Text } from '../components/Text'
import { prefs, toggled, usePref } from '../lib/prefs'
import { useTr } from '../lib/use-language'
import { useScreenViewed } from '../store/analytics'
import { refKey, sameRef, type BotRef, type RosterItem } from '../store/app-model'
import { StoreContext, useStore } from '../store/context'
import { relativeTime } from './chat/chat-items'
import { CutInDialog, StopWorkDialog, WORK_MARK, WorkDot } from './chat/WorkDialogs'
import { threadMenuItems } from './thread-menu'
import { ListSectionHeading } from './ListSectionHeading'
import { ThreadChat } from './chat/ThreadChat'
import { ThreadStart, type ThreadWith } from './chat/ThreadStart'
import './computer-roster.css'
import './threads-page.css'

/** The thread the Threads page shows: a task of a bot's. */
export interface ThreadPick {
  ref: BotRef
  taskId: string
}

const SHOWN_RECENT = 8

/**
 * The Threads page's list, in the shape of a chat app's sidebar (its header's `+` starts a thread): what needs the user, then
 * the groups the list's options make (`threadsGroup`: the folders threads work in outside their bot's own and the
 * routines' runs, a group per bot in the bot list's order, or one per computer), then the rest, newest first, the last of it on demand.
 * A row is a thread's title and one mark at its end for how it stands; a group's heading folds it, and a bot's has a
 * `+` that starts a thread with it (`start`). A row of a task a thread has or had shows its thread beside the list
 * (`pick`). A row's `…` (shown on hover) and its right-click open its menu (`threadMenuItems`): the thread, its bot's
 * chat (`openChat`), another thread with the bot, what its bot's Details does with it, and copying its title or folder.
 */
export function ThreadList({ roster, picked, pick, start, openChat }: {
  roster: RosterItem[]
  picked: ThreadPick | null
  pick: (thread: ThreadPick) => void
  start: (ref: BotRef | null) => void
  /** Opens a bot's chat on the Bots page. */
  openChat: (ref: BotRef) => void
}) {
  const tr = useTr()
  useScreenViewed('all_work')
  const [grouping] = usePref(prefs.threadsGroup)
  const [byStatus] = usePref(prefs.threadsByStatus)
  const [byActivity] = usePref(prefs.threadsByActivity)
  const [folded, setFolded] = usePref(prefs.foldedThreadSections)
  const [more, setMore] = useState(false)
  // A thread's menu asks before Stop and Cut in, with its bot's dialogs.
  const [asking, setAsking] = useState<{ of: RosterItem; task: WorkTask; kind: 'stop' | 'cutIn' } | null>(null)
  const { needsYou, groups, recent } = threadSections(roster, grouping, (of) => refKey(of.ref), (of) => of.store.hostName, { status: byStatus, active: byActivity })
  const row = ({ of, task }: { of: RosterItem; task: WorkTask }, label = task.title) => (
    <StoreContext.Provider key={`${refKey(of.ref)}/${task.id}`} value={of.store}>
      <ThreadRow
        flip={`${refKey(of.ref)}/${task.id}`}
        task={task}
        agent={agentOf(task, of.bot)}
        label={label}
        selected={picked !== null && picked.taskId === task.id && sameRef(picked.ref, of.ref)}
        open={task.thread ? () => pick({ ref: of.ref, taskId: task.id }) : undefined}
        items={() => threadMenuItems(of.bot, task, threadBots([of]).length > 0 || plainStores([of]).length > 0, {
          open: () => pick({ ref: of.ref, taskId: task.id }),
          openChat: () => openChat(of.ref),
          startAnother: () => start(of.ref),
          stop: () => setAsking({ of, task, kind: 'stop' }),
          cutIn: () => setAsking({ of, task, kind: 'cutIn' }),
          first: () => of.store.workFirst(of.bot.id, task.id),
          retry: () => of.store.retryWork(of.bot.id, task.id),
          drop: () => of.store.dropWork(of.bot.id, task.id),
          copy: (text) => window.codync.app.copy(text),
        })}
      />
    </StoreContext.Provider>
  )
  const label = (title: string) => <Text as="div" size="caption" tone="tertiary" className="thread-label">{tr(title)}</Text>
  const shownRecent = more ? recent : recent.slice(0, SHOWN_RECENT)
  // Older threads: each computer's first page once the list shows, the next as its end scrolls into view.
  const stores = [...new Set(roster.map((r) => r.store))]
  useEffect(() => stores.forEach((s) => s.loadFirstOlderThreads()))
  return (
    <>
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
              // The box of threads without a bot is no bot: its group says so.
              const title = of.bot.plain && group.kind === 'bot' ? tr('No bot') : group.key === OWN_FOLDER_GROUP ? tr(group.title) : group.title
              // Folded, its heading still says that something in it went wrong or works.
              const loud = isFolded ? (group.rows.find((r) => r.task.status === 'failed') ?? group.rows.find((r) => r.task.status === 'working')) : undefined
              return (
                <div key={group.key} className="thread-group" role="group" aria-label={title}>
                  <ListSectionHeading
                    leading={group.kind === 'bot' ? (of.bot.plain ? <NoBotAvatar size={16} /> : <BotAvatar bot={of.bot} size={16} animated={false} />) : <Icon name={({ folder: 'folder', routine: 'clock', computer: 'desktopcomputer' } as const)[group.kind]} size={12} color="var(--secondary)" />}
                    title={title}
                    mark={loud ? <StatusMark status={loud.task.status} /> : null}
                    folded={isFolded}
                    onFold={() => setFolded(toggled(folded, group.key))}
                    add={group.kind === 'bot' && (threadBots([of]).length || plainStores([of]).length) ? { title: 'New thread', action: () => start(of.ref) } : null}
                  />
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
      {recent.length > shownRecent.length ? null : <ListEnd reached={() => stores.forEach((s) => s.loadOlderThreads())} />}
      {asking ? (
        <StoreContext.Provider value={asking.of.store}>
          <StopWorkDialog botId={asking.of.bot.id} taskId={asking.kind === 'stop' ? asking.task.id : null} onClose={() => setAsking(null)} />
          <CutInDialog bot={asking.of.bot} task={asking.kind === 'cutIn' ? asking.task : null} onClose={() => setAsking(null)} />
        </StoreContext.Provider>
      ) : null}
    </>
  )
}

/** The list's end: `reached` while it is in view (watched again on each render, so a page that leaves it in view asks for the next). */
function ListEnd({ reached }: { reached: () => void }) {
  const end = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const seen = new IntersectionObserver((at) => at.some((e) => e.isIntersecting) && reached())
    if (end.current) seen.observe(end.current)
    return () => seen.disconnect()
  }, [reached])
  return <div ref={end} aria-hidden />
}

/** How a thread stands, at the end of its row: working, needs the user, went wrong, waits; nothing once it is done. */
function StatusMark({ status }: { status: WorkTask['status'] }) {
  return status === 'done' ? null : <WorkDot mark={WORK_MARK[status]} />
}

/** One thread of the list: the coding agent it runs on, its title (a routine's run: when it started) and its mark, whose place a `…` takes while the
 * row is hovered; the `…` and a right-click open its menu. */
function ThreadRow({ flip, task, agent, label, selected, open, items }: { flip: string; task: WorkTask; agent: string | null; label: string; selected: boolean; open?: () => void; items: () => MenuItem[] }) {
  const tr = useTr()
  const store = useStore()
  const context = useContextMenu()
  const moreRef = useRef<HTMLButtonElement>(null)
  const [more, setMore] = useState(false)
  const Open = open ? 'button' : 'div'
  return (
    <div className={`thread-item ${more || context.open ? 'menu-open' : ''}`} data-flip={flip} onContextMenu={context.onContextMenu}>
      <Open className={`thread-row ${selected ? 'selected' : ''}`} data-thread-row={task.title} data-work-status={task.status} onClick={open} title={task.title}>
        <span className="thread-agent">{agent ? <AgentIcon registry={store.agentRegistry(agent)} size={14} /> : null}</span>
        <Text tone="text" truncate grow>{label}</Text>
        <span className="thread-mark"><StatusMark status={task.status} /></span>
      </Open>
      <IconButton
        className="thread-more"
        title={tr('More actions for {name}').replaceAll('{name}', () => task.title)}
        icon="ellipsis"
        size={24}
        buttonRef={moreRef}
        menuOpen={more}
        disabled={store.isOffline}
        onClick={() => setMore(true)}
      />
      <AnchoredMenu open={more} onClose={() => setMore(false)} anchor={moreRef} items={items} />
      <AnchoredMenu open={context.open} onClose={context.close} point={context.point} items={items} />
    </div>
  )
}

/** The bots a thread can start with: on a computer that is connected and starts threads; never a group, nor the box
 * of threads without a bot. */
export const threadBots = (roster: RosterItem[]) => roster.filter((r) => !isGroup(r.bot) && !r.bot.plain && r.store.startsThreads && !r.store.isOffline)

/** Whether a thread can start at all: with a bot, or on a computer that starts one without. */
export const canStartThread = (roster: RosterItem[]) => threadBots(roster).length > 0 || plainStores(roster).length > 0

/** The connected computers of `roster` where a thread may start without a bot, one item each. */
const plainStores = (roster: RosterItem[]) =>
  roster.filter((r, at) => r.store.startsThreadsWithoutBot && !r.store.isOffline && roster.findIndex((o) => o.ref.computerId === r.ref.computerId) === at)

/**
 * Beside the list: the thread picked there, in the chat's form (`ThreadChat`); one that still waits for a place says
 * so. With none picked,
 * or one whose task left its bot's box, the page a new thread starts from (`NewThread`, with the bot `starting`
 * names, else no bot where the computer allows it, else the lead bot), or the page's mark where no bot can start one.
 */
export function ThreadDetail({ roster, picked, pick, starting, start, openChat }: {
  roster: RosterItem[]
  picked: ThreadPick | null
  pick: (thread: ThreadPick) => void
  starting: BotRef | null
  start: (ref: BotRef) => void
  /** Opens a bot's chat: where its panel's cards and entries are shown. */
  openChat: (ref: BotRef) => void
}) {
  const tr = useTr()
  const of = picked ? roster.find((r) => sameRef(r.ref, picked.ref)) : undefined
  const task = picked ? of?.bot.work.find((i) => i.id === picked.taskId) : undefined
  if (of && task?.thread) {
    return (
      <StoreContext.Provider value={of.store}>
        <ThreadChat key={`${refKey(of.ref)}/${task.id}`} bot={of.bot} task={task} openChat={() => openChat(of.ref)} />
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
  // A thread starts with the bot `starting` names; otherwise (the header's +, or `botId` '' or the box of threads
  // without a bot) with none, on its computer where it can, else the first computer that can.
  const plainOn = plainStores(roster)
  const chosen = bots.find((r) => sameRef(r.ref, starting))
  const plain = chosen ? undefined : plainOn.find((r) => r.ref.computerId === starting?.computerId) ?? plainOn[0]
  const first: ThreadWith | undefined = chosen
    ?? (plain ? { ref: { computerId: plain.ref.computerId, botId: '' }, store: plain.store, bot: null } : bots.find((r) => r.bot.lead) ?? bots[0])
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
      <ThreadStart
        key={refKey(first.ref)}
        bots={bots}
        computers={plainOn}
        of={first}
        choose={(ref, none) => start(none ? { computerId: ref.computerId, botId: '' } : ref)}
        started={(ref, taskId) => pick({ ref, taskId })}
      />
    </StoreContext.Provider>
  )
}
