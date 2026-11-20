import { hasLibrary } from '@shared/artifacts'
import { isGroup, isWorking, type Bot } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { IconButton, TabChoice } from '../../components/Controls'
import { LeadAgentBadge } from '../../components/LeadAgentBadge'
import { Icon } from '../../components/Icon'
import { VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { BotSettingsPanel } from '../bots/BotEditorView'
import { MemoryCard } from '../bots/MemoryView'
import { RoutinesView } from '../routines/RoutinesView'
import { LibraryView } from '../library/LibraryView'
import { DevServersSection } from './DevServersSection'
import { ComputerTab } from './ComputerTab'
import { WorkThread } from './WorkThread'
import { WorkTimeline } from './WorkTimeline'
import { MacChatButton } from './MacChatButton'
import { useThreadPage } from './use-thread-page'
import '../bots/bots.css'
import './details-panel.css'
import { useTr } from '../../lib/use-language'


export type DetailsTab = 'details' | 'computer' | 'library' | 'memory' | 'settings'

const TABS: { id: DetailsTab; label: string }[] = [
  { id: 'details', label: 'Details' },
  { id: 'computer', label: 'Computer' },
  { id: 'library', label: 'Library' },
  { id: 'memory', label: 'Memory' },
  { id: 'settings', label: 'Settings' },
]

/** The inspector beside a conversation (Grok Bot's): who the bot is, then what it does (Details), its computer, library,
 * memory and how it is set up (Settings). A row of a task a thread has slides the panel to that task's thread
 * (`thread`); its Back returns to the page as it was left. */
export function DetailsPanel({ botId, routineId, tab, setTab, thread, openThread, closeThread, editGroup, openArtifact, openCard, showInChat, share, close }: {
  botId: string
  routineId: string | null
  tab: DetailsTab
  setTab: (tab: DetailsTab) => void
  /** The task whose thread shows in place of the pages; null for the pages. */
  thread: string | null
  openThread: (taskId: string) => void
  closeThread: () => void
  editGroup: () => void
  openArtifact: (artifactId: string) => void
  /** Shows the chat at the approval card of a task that needs the user. */
  openCard: (taskId: string) => void
  /** Shows the chat at one of its entries: what a thread put there. */
  showInChat: (entryId: string) => void
  share: () => void
  close: () => void
}) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const group = !!bot && isGroup(bot)
  useScreenViewed('bot_details')
  const { reading, page, shown, opening } = useThreadPage(thread, bot, closeThread)
  return (
    <div className="details-panel">
      <div className={reading ? 'details-page away' : 'details-page'} inert={reading}>
        <div className="details-drag" />
        <div className="details-actions">
          {bot && !group ? <MacChatButton className="ringed" title="Create template" icon="square.and.arrow.up" onClick={share} /> : null}
          <MacChatButton className="ringed" title="Close details" icon="xmark" onClick={close} />
        </div>
        {/* The column is the sheet's scroller, so it takes the focus when the sheet opens: the card around it
            never scrolls, and a first PageDown aimed at the card would move nothing. */}
        <div className="details-scroll" tabIndex={-1} data-autofocus>
          {bot ? <Identity bot={bot} /> : null}
          {bot && group ? (
            <GroupMembersList group={bot} editGroup={editGroup} />
          ) : (
            <>
              <TabChoice label="Conversation details" selection={tab} options={hasLibrary(store.hostVersion?.version) ? TABS : TABS.filter((t) => t.id !== 'library')} onChange={setTab} />
              <div key={tab} className="details-tab fade-in">
                {tab === 'details' ? (
                  bot ? (
                    <WorkTimeline
                      bot={bot}
                      openCard={openCard}
                      openThread={(taskId) => {
                        opening()
                        openThread(taskId)
                      }}
                    >
                      <RoutinesView botId={botId} initialId={routineId} />
                      <DevServersSection bot={bot} />
                    </WorkTimeline>
                  ) : null
                ) : tab === 'computer' ? (
                  bot ? <ComputerTab bot={bot} /> : null
                ) : tab === 'library' ? (
                  <LibraryView botId={botId} openArtifact={openArtifact} />
                ) : tab === 'memory' ? (
                  <MemoryCard botId={botId} />
                ) : (
                  <BotSettingsPanel botId={botId} />
                )}
              </div>
            </>
          )}
        </div>
      </div>
      {page.mounted && shown ? (
        <div className={page.shown && reading ? 'details-page thread shown' : 'details-page thread'} inert={!reading}>
          <WorkThread key={shown.id} botId={botId} task={shown} openArtifact={openArtifact} showInChat={showInChat} close={closeThread} />
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
