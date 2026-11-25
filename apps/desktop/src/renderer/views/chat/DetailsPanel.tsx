import { hasLibrary } from '@shared/artifacts'
import { closeView, hasTerminal, isThreadTab, multiView, PANEL_VIEWS, panelLayout, showChooser, showView, threadTab, toggleSplit, type PanelView } from '@shared/panel-layout'
import { isGroup, isWorking, type Bot } from '@shared/models'
import type { WorkTask } from '@shared/work'
import type { ReactNode } from 'react'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { IconButton } from '../../components/Controls'
import { analytics, useScreenViewed } from '../../store/analytics'
import { LeadAgentBadge } from '../../components/LeadAgentBadge'
import { Icon } from '../../components/Icon'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { BotSettingsPanel } from '../bots/BotEditorView'
import { MemoryCard } from '../bots/MemoryView'
import { RoutinesView } from '../routines/RoutinesView'
import { LibraryView } from '../library/LibraryView'
import { DevServersSection } from './DevServersSection'
import { closeTerminal } from './TerminalTab'
import { ComputerTab } from './ComputerTab'
import { MultiTab, multiTabViews, PanelPane, type PaneView } from './PanelPane'
import { ThreadChat } from './ThreadChat'
import { NO_BAR_SLOTS, WindowBarSlots } from '../WindowBar'
import { WorkTimeline } from './WorkTimeline'
import '../bots/bots.css'
import './details-panel.css'
import { prefs, usePref } from '../../lib/prefs'
import { useTr } from '../../lib/use-language'

export type DetailsTab = PanelView

/** The bot's own chat, a view only beside one of its threads. */
const SIDE_CHAT = { id: 'chat', label: 'Side chat', icon: 'bubble.left.and.bubble.right', key: 'h' }

/** Every view, and the threads' tabs (the panel itself knows which are this bot's). */
const anyTab = (tab: string) => isThreadTab(tab) || tab === SIDE_CHAT.id || PANEL_VIEWS.some((v) => v.id === tab)

/** Shows `view` of the panel, in the pane that has its tab or as a new tab of the first. */
export const showPanelView = (view: PanelView | 'chat') => prefs.panelLayout.set(showView(panelLayout(prefs.panelLayout.get(), anyTab), view))

/** Splits the panel into two panes, or joins them. */
export function splitPanel() {
  const next = toggleSplit(panelLayout(prefs.panelLayout.get(), anyTab))
  analytics.track({ event: 'panel_split', on: next.length > 1 })
  prefs.panelLayout.set(next)
}

/**
 * The panel beside a conversation: one pane, or two with the second under the first (Split panel). A pane has the
 * bot's views as tabs (what it does in Details, its computer, library and memory, how it is set up in Settings, a web
 * page in Browser), each closing on its own, and `+` for its chooser, which is all a new panel shows. A view has one
 * tab, so opening it in a pane takes it from the other. The layout is the same for every bot on this device. Docked,
 * the window's bar holds the panel's buttons; as a sheet (`docked` false) they end its first row. A row of a task a
 * thread has opens that thread as a tab of its pane, named by the task, beside the views; it closes like them and goes
 * when its task leaves the box. A group has no panes: its avatar, then its bots. Beside one of the bot's threads
 * (`thread`) the panel leaves that thread's own tab out, its Terminal opens in the thread's folder, and `sideChat`
 * (the bot's chat) is one more view.
 */
export function DetailsPanel({ botId, routineId, editGroup, openArtifact, openCard, share, close, docked, thread, sideChat }: {
  botId: string
  routineId: string | null
  editGroup: () => void
  openArtifact: (artifactId: string) => void
  /** Shows the chat at the approval card of a task that needs the user. */
  openCard: (taskId: string) => void
  share: () => void
  close: () => void
  /** Beside the conversation, under the window's bar; false for the sheet of a narrow window. */
  docked: boolean
  /** The thread the panel is beside, when it is beside one. */
  thread?: WorkTask
  /** The bot's chat, as a view beside `thread`. */
  sideChat?: ReactNode
}) {
  const tr = useTr()
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const group = !!bot && isGroup(bot)
  useScreenViewed('bot_details')
  // A view this computer's Codync has no page for (the Library or the Terminal, on an older host) is not offered.
  const host = store.hostVersion?.version
  const own = PANEL_VIEWS.filter((v) => (v.id !== 'library' || hasLibrary(host)) && (v.id !== 'terminal' || hasTerminal(host)))
  // The bot's threads open as tabs too, named by their task; a thread's tab goes with its task.
  const threads = (bot?.work ?? []).filter((t) => t.thread && t.id !== thread?.id)
  const fixed: PaneView[] = [...own, ...(sideChat ? [SIDE_CHAT] : []), ...threads.map((t) => ({ id: threadTab(t.id), label: t.title, icon: 'arrow.triangle.branch', verbatim: true }))]
  const [stored, setStored] = usePref(prefs.panelLayout)
  const layout = panelLayout(stored, (tab) => fixed.some((v) => v.id === tab))
  // Browsers and terminals open as many tabs as are picked.
  const views = [...fixed, ...multiTabViews(layout.flatMap((p) => p.tabs), own, tr)]
  const view = (id: string, at: number) =>
    !bot ? null : id === 'details' ? (
      <WorkTimeline bot={bot} openCard={openCard} openThread={(taskId) => setStored(showView(layout, threadTab(taskId), at))}>
        <RoutinesView botId={botId} initialId={routineId} />
        <DevServersSection bot={bot} />
      </WorkTimeline>
    ) : id === 'computer' ? (
      <ComputerTab bot={bot} />
    ) : id === 'library' ? (
      <LibraryView botId={botId} openArtifact={openArtifact} />
    ) : id === 'memory' ? (
      <MemoryCard botId={botId} />
    ) : (
      <BotSettingsPanel botId={botId} />
    )
  const tabTask = (tab: string | null) => (tab && isThreadTab(tab) ? threads.find((t) => threadTab(t.id) === tab) : undefined)
  return (
    <div className="details-panel">
      <div className="details-page">
        {bot && group ? (
          <>
            {docked ? null : <div className="details-actions"><IconButton title="Close details" icon="xmark" size={26} onClick={close} /></div>}
            <div className="details-scroll" tabIndex={-1} data-autofocus>
              <Identity bot={bot} />
              <GroupMembersList group={bot} editGroup={editGroup} />
            </div>
          </>
        ) : bot ? (
          <>
            <div className="details-panes">
              {layout.map((pane, at) => {
                const task = tabTask(pane.shown)
                const multi = pane.shown ? multiView(pane.shown) : null
                return (
                <PanelPane
                  key={at}
                  pane={pane}
                  views={views}
                  first={at === 0}
                  actions={at > 0 || docked ? undefined : <>
                    <IconButton title="Create template" icon="square.and.arrow.up" size={26} onClick={share} />
                    <IconButton title="Split panel" icon="rectangle.split.1x2" size={26} onClick={splitPanel} />
                    <IconButton title="Close details" icon="xmark" size={26} onClick={close} />
                  </>}
                  flush={!!multi || pane.shown === SIDE_CHAT.id || !!task}
                  show={(id) => setStored(showView(layout, id, at))}
                  close={(id) => {
                    closeTerminal(store.client, id)
                    setStored(closeView(layout, at, id))
                  }}
                  choose={() => setStored(showChooser(layout, at))}
                >
                  {multi ? <MultiTab bot={bot} thread={thread?.thread} tab={pane.shown!} />
                    : pane.shown === SIDE_CHAT.id ? sideChat
                    : task ? /* No window's bar here: the thread's status shows in the tab. */ <WindowBarSlots.Provider key={task.id} value={NO_BAR_SLOTS}><ThreadChat bot={bot} task={task} openChat={() => openCard(task.id)} /></WindowBarSlots.Provider>
                    : pane.shown ? <div key={pane.shown} className="details-tab fade-in">{view(pane.shown, at)}</div> : null}
                </PanelPane>
                )
              })}
            </div>
          </>
        ) : null}
      </div>
    </div>
  )
}

/** The bot's avatar and name, and what runs it (a group: how many bots it has). */
function Identity({ bot }: { bot: Bot }) {
  const store = useStore()
  const group = isGroup(bot)
  return (
    <div className="details-identity">
      {group ? <GroupAvatar members={store.members(bot)} size={48} /> : <div className="avatar-frame"><BotAvatar bot={bot} size={48} />{bot.lead ? <LeadAgentBadge size={24} interactive /> : null}</div>}
      <span className="details-name">{bot.name}</span>
      <span className="details-subtitle">
        {group ? (
          `${bot.members.length} ${bot.members.length === 1 ? 'bot' : 'bots'}`
        ) : (
          <>
            <AgentIcon registry={store.agentRegistry(bot.backend)} size={12} />
            {store.backendName(bot.backend)}
            {bot.model ? ` · ${bot.model}` : null}
          </>
        )}
      </span>
    </div>
  )
}

/** A group's bots (Grok Bot's member list); clicking one opens its own chat. */
function GroupMembersList({ group, editGroup }: { group: Bot; editGroup: () => void }) {
  const tr = useTr()
  const store = useStore()
  return (
    <div className="details-members">
      {store.members(group).map((bot) => (
        <div key={bot.id} className="member-row removable">
          <button className="member-open press" title={tr("Open {name}'s own chat").replaceAll('{name}', () => bot.name)} onClick={() => store.setSelection(bot.id)}>
            <BotAvatar bot={bot} size={32} />
            <VStack as="span" gap={1} grow>
              <span className="member-name">{bot.name}</span>
              {isWorking(bot) ? <Text size="compactSecondary" tone="secondary" className="member-name">{tr(bot.activity || 'Working…')}</Text> : null}
            </VStack>
          </button>
          {group.members.length > 1 ? <IconButton title={tr('Remove {name} from the group').replaceAll('{name}', () => bot.name)} icon="xmark" onClick={() => store.removeMember(group, bot.id)} /> : null}
        </div>
      ))}
      <button className="member-row press" onClick={editGroup}>
        <span className="member-add-icon">
          <Icon name="plus" size={16} color="var(--secondary)" />
        </span>
        <Text tone="secondary" className="member-name">{tr(`Add Member`)}</Text>
      </button>
    </div>
  )
}
