import { hasLibrary } from '@shared/artifacts'
import { isGroup, isWorking, type Bot } from '@shared/models'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar, GroupAvatar } from '../../components/Avatar'
import { IconButton, RowLabel, Switch, TabChoice } from '../../components/Controls'
import { LeadAgentBadge } from '../../components/LeadAgentBadge'
import { Icon } from '../../components/Icon'
import { HStack, VStack } from '../../components/Stack'
import { Text } from '../../components/Text'
import { useStore } from '../../store/context'
import { useScreenViewed } from '../../store/analytics'
import { BotSettingsPanel } from '../bots/BotEditorView'
import { MemoryCard } from '../bots/MemoryView'
import { RoutinesView } from '../routines/RoutinesView'
import { LibraryView } from '../library/LibraryView'
import { MacChatButton } from './MacChatButton'
import '../bots/bots.css'
import './details-panel.css'
import { useTr } from '../../lib/use-language'


export type DetailsTab = 'details' | 'library' | 'memory' | 'settings'

const TABS: { id: DetailsTab; label: string }[] = [
  { id: 'details', label: 'Details' },
  { id: 'library', label: 'Library' },
  { id: 'memory', label: 'Memory' },
  { id: 'settings', label: 'Settings' },
]

/** The inspector beside a conversation (Grok Bot's): who the bot is, then its details, library, memory and settings. */
export function DetailsPanel({ botId, routineId, tab, setTab, editGroup, openArtifact, share, close }: {
  botId: string
  routineId: string | null
  tab: DetailsTab
  setTab: (tab: DetailsTab) => void
  editGroup: () => void
  openArtifact: (artifactId: string) => void
  share: () => void
  close: () => void
}) {
  const store = useStore()
  const bot = store.bots.get(botId) ?? null
  const group = !!bot && isGroup(bot)
  useScreenViewed('bot_details')
  return (
    <div className="details-panel">
      <div className="details-drag" />
      <div className="details-actions">
        {bot && !group ? <MacChatButton className="ringed" title="Create template" icon="square.and.arrow.up" onClick={share} /> : null}
        <MacChatButton className="ringed" title="Close details" icon="xmark" onClick={close} />
      </div>
      <div className="details-scroll">
        {bot ? <Identity bot={bot} /> : null}
        {bot && group ? (
          <GroupMembersList group={bot} editGroup={editGroup} />
        ) : (
          <>
            <TabChoice label="Conversation details" selection={tab} options={hasLibrary(store.hostVersion?.version) ? TABS : TABS.filter((t) => t.id !== 'library')} onChange={setTab} />
            <div key={tab} className="details-tab fade-in">
              {tab === 'details' ? (
                <>
                  <BotSettingsPanel botId={botId} part="often" />
                  {bot ? <ComputerUse bot={bot} /> : null}
                  <RoutinesView botId={botId} initialId={routineId} />
                </>
              ) : tab === 'library' ? (
                <LibraryView botId={botId} openArtifact={openArtifact} />
              ) : tab === 'memory' ? (
                <MemoryCard botId={botId} />
              ) : (
                <BotSettingsPanel botId={botId} part="rarely" />
              )}
            </div>
          </>
        )}
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

/** Whether this bot may use the computer, switched right here (also in Settings). */
function ComputerUse({ bot }: { bot: Bot }) {
  const tr = useTr()
  const store = useStore()
  const screen = store.screen
  if (!screen) return null
  const ready = screen.platform === 'windows' || (screen.enabled && screen.connected && screen.capture && screen.input)
  return (
    <section className="details-section">
      <Text className="details-section-title">{tr(`Computer`)}</Text>
      <div className="outlined-card details-switch">
        <RowLabel grow label="Use the computer" detail={ready ? null : 'On that computer, open Codync → Settings → Computer access to set it up first.'} />
        <HStack as="span" align="stretch" className="details-switch-control">
          <Switch label="Use the computer" on={bot.computer} disabled={store.isOffline} onChange={(on) => store.setComputer(bot, on)} />
        </HStack>
      </div>
    </section>
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
          <button className="member-open press" title={`Open ${bot.name}'s own chat`} onClick={() => store.setSelection(bot.id)}>
            <BotAvatar bot={bot} size={32} />
            <VStack as="span" gap={1} grow>
              <span className="member-name">{bot.name}</span>
              {isWorking(bot) ? <Text size="compactSecondary" tone="secondary" className="member-name">{tr(bot.activity || 'Working…')}</Text> : null}
            </VStack>
          </button>
          {group.members.length > 1 ? <IconButton title={`Remove ${bot.name} from the group`} icon="xmark" onClick={() => store.removeMember(group, bot.id)} /> : null}
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
