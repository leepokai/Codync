import { hasLibrary } from '@shared/artifacts'
import { closeView, PANEL_VIEWS, panelLayout, showChooser, showView, toggleSplit, type PanelLayout, type PanelView } from '@shared/panel-layout'
import { isGroup, isWorking, type Bot } from '@shared/models'
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
import { BrowserTab } from './BrowserTab'
import { ComputerTab } from './ComputerTab'
import { PanelPane } from './PanelPane'
import { WorkThread } from './WorkThread'
import { WorkTimeline } from './WorkTimeline'
import { MacChatButton } from './MacChatButton'
import { useThreadPage } from './use-thread-page'
import '../bots/bots.css'
import './details-panel.css'
import { prefs, usePref } from '../../lib/prefs'
import { useTr } from '../../lib/use-language'

export type DetailsTab = PanelView

const EVERY_VIEW = PANEL_VIEWS.map((v) => v.id)

/** Shows `view` of the panel, in the pane that has its tab or as a new tab of the first. */
export const showPanelView = (view: PanelView) => prefs.panelLayout.set(showView(panelLayout(prefs.panelLayout.get(), EVERY_VIEW), view))

/**
 * The panel beside a conversation: the bot and the panel's own buttons on top, then one pane, or two with the second
 * under the first (Split panel). A pane has the bot's views as tabs (what it does in Details, its computer, library and
 * memory, how it is set up in Settings, a web page in Browser), each closing on its own, and `+` for its chooser. A
 * view has one tab, so opening it in a pane takes it from the other. The layout is the same for every bot on this
 * device. `maximize` (docked only) lets the panel take the conversation's place. A row of an item a task has slides the
 * panel to that item's thread (`thread`); its Back returns to the panes as they were left. A group has no panes: its
 * avatar, then its bots.
 */
export function DetailsPanel({ botId, routineId, thread, openThread, closeThread, editGroup, openArtifact, openCard, showInChat, share, close, maximized, maximize }: {
  botId: string
  routineId: string | null
  /** The item whose thread shows in place of the panes; null for the panes. */
  thread: string | null
  openThread: (itemId: string) => void
  closeThread: () => void
  editGroup: () => void
  openArtifact: (artifactId: string) => void
  /** Shows the chat at the approval card of an item that needs the user. */
  openCard: (itemId: string) => void
  /** Shows the chat at one of its entries: what a thread's task put there. */
  showInChat: (entryId: string) => void
  share: () => void
  close: () => void
  maximized: boolean
  /** Null where the panel has no conversation beside it to cover (the sheet of a narrow window). */
  maximize: (() => void) | null
}) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const group = !!bot && isGroup(bot)
  useScreenViewed('bot_details')
  const { reading, page, shown, opening } = useThreadPage(thread, bot, closeThread)
  // A view this computer's Codync has no page for (the Library, on an older host) is not offered.
  const views = hasLibrary(store.hostVersion?.version) ? PANEL_VIEWS : PANEL_VIEWS.filter((v) => v.id !== 'library')
  const [stored, setStored] = usePref(prefs.panelLayout)
  const layout = panelLayout(stored, views.map((v) => v.id))
  const split = (next: PanelLayout) => {
    analytics.track({ event: 'panel_split', on: next.length > 1 })
    setStored(next)
  }
  const view = (id: string) =>
    !bot ? null : id === 'details' ? (
      <WorkTimeline
        bot={bot}
        openCard={openCard}
        openThread={(itemId) => {
          opening()
          openThread(itemId)
        }}
      >
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
  return (
    <div className="details-panel">
      <div className={reading ? 'details-page away' : 'details-page'} inert={reading}>
        <div className="details-drag" />
        <div className="details-actions">
          {bot && !group ? <MacChatButton className="ringed" title="Split panel" icon="rectangle.split.1x2" onClick={() => split(toggleSplit(layout))} /> : null}
          {maximize ? <MacChatButton className="ringed" title="Maximize panel" icon={maximized ? 'arrow.down.right.and.arrow.up.left' : 'arrow.up.left.and.arrow.down.right'} onClick={maximize} /> : null}
          {bot && !group ? <MacChatButton className="ringed" title="Create template" icon="square.and.arrow.up" onClick={share} /> : null}
          <MacChatButton className="ringed" title="Close details" icon="xmark" onClick={close} />
        </div>
        {bot && group ? (
          <div className="details-scroll" tabIndex={-1} data-autofocus>
            <Identity bot={bot} />
            <GroupMembersList group={bot} editGroup={editGroup} />
          </div>
        ) : bot ? (
          <>
            <div className="details-who">
              <BotAvatar bot={bot} size={24} />
              <Text size="body" weight="semibold" tone="text" truncate>{bot.name}</Text>
            </div>
            <div className="details-panes">
              {layout.map((pane, at) => (
                <PanelPane
                  key={at}
                  pane={pane}
                  views={views}
                  first={at === 0}
                  flush={pane.shown === 'browser'}
                  show={(id) => setStored(showView(layout, id, at))}
                  close={(id) => setStored(closeView(layout, at, id))}
                  choose={() => setStored(showChooser(layout, at))}
                >
                  {pane.shown === 'browser' ? <BrowserTab key={bot.id} bot={bot} /> : pane.shown ? <div key={pane.shown} className="details-tab fade-in">{view(pane.shown)}</div> : null}
                </PanelPane>
              ))}
            </div>
          </>
        ) : null}
      </div>
      {page.mounted && shown ? (
        <div className={page.shown && reading ? 'details-page thread shown' : 'details-page thread'} inert={!reading}>
          <WorkThread key={shown.id} botId={botId} item={shown} openArtifact={openArtifact} showInChat={showInChat} close={closeThread} />
        </div>
      ) : null}
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
