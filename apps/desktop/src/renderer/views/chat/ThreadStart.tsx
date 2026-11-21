import { useRef, useState, type ReactNode } from 'react'
import { approvals, withApproval } from '@shared/approvals'
import { folderName, type Bot } from '@shared/models'
import type { ThreadSetup } from '@shared/work'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar } from '../../components/Avatar'
import { DropdownMenu } from '../../components/Controls'
import { Icon } from '../../components/Icon'
import type { MenuItem } from '../../components/Menu'
import { Sheet } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { refKey, sameRef, type BotRef, type RosterItem } from '../../store/app-model'
import type { BotStore } from '../../store/bot-store'
import { modelChoices, useAgentModels } from '../bots/agent-models'
import { lastPathComponent } from '../bots/drafts'
import { FolderPicker } from '../bots/FolderPicker'
import { ChatLog, useDockHeight } from './ChatLog'
import { Composer } from './Composer'
import './chat.css'
import './composer.css'
import './chat-controls.css'
import './thread-chat.css'

/** Who a new thread is with: a bot of `store`'s computer, or none (`bot` null, or the computer's box of threads
 * without a bot), which runs an agent on its own. */
export interface ThreadWith {
  ref: BotRef
  store: BotStore
  bot: Bot | null
}

/** How a thread without a bot is approved until the user picks: it asks, in the agent's Auto mode, as a new bot. */
const PLAIN_APPROVAL = { permission: 'ask', mode: 'auto' }

/**
 * The page a thread starts from, in the form of a chat nothing was said in yet: the bot it is with in the middle, and
 * the chat's message box. Over the box, what the thread runs on, each a menu: the bot (or **No bot**, where the
 * computer can), which the others follow (they start as that bot's own), then the folder it works in, how it is
 * approved and its model with that model's settings. Without a bot the agent is chosen too, the folder starts as
 * the home folder and nothing of Codync's goes with it. A choice is the thread's alone (`ThreadSetup`): the bot's
 * setup stays as it is. Sent, the thread is the one shown (`started`, with where its task is).
 * ponytail: the model menu lists every model without a search; the bot's settings have one if lists grow long.
 */
export function ThreadStart({ bots, of, choose, started }: { bots: RosterItem[]; of: ThreadWith; choose: (ref: BotRef, plain?: boolean) => void; started: (ref: BotRef, taskId: string) => void }) {
  const tr = useTr()
  const { store } = of
  const bot = of.bot?.plain ? null : of.bot
  const [setup, setSetup] = useState<ThreadSetup>({})
  const change = (part: ThreadSetup) => setSetup((s) => ({ ...s, ...part }))
  const [failed, setFailed] = useState('')
  const [browsing, setBrowsing] = useState(false)
  const agents = (store.hello?.backends ?? []).filter((b) => b.available)
  const backend = setup.backend ?? bot?.backend ?? agents.find((b) => b.id === 'claude')?.id ?? agents[0]?.id ?? ''
  const models = useAgentModels(backend)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)

  const many = new Set(bots.map((r) => r.ref.computerId)).size > 1
  const plainOn = store.startsThreadsWithoutBot
  const others = (): MenuItem[] => [
    ...bots.map((r) => ({
      title: r.bot.name, verbatim: true, detail: many ? r.store.hostName : undefined,
      leading: <BotAvatar bot={r.bot} size={22} animated={false} />, selected: bot !== null && sameRef(r.ref, of.ref), action: () => choose(r.ref),
    })),
    ...(plainOn ? [{ title: 'No bot', icon: 'cpu', divider: bots.length > 0, selected: bot === null, action: () => choose(of.ref, true) }] : []),
  ]

  const home = store.hello?.home ?? ''
  const ownFolder = bot ? folderName(bot, tr) : lastPathComponent(home) || home
  const folders = (): MenuItem[] => [
    { title: ownFolder, verbatim: true, detail: bot?.managedWorkspace ? undefined : (bot?.cwd ?? home), selected: !setup.cwd, action: () => change({ cwd: undefined }) },
    ...(setup.cwd ? [{ title: lastPathComponent(setup.cwd), verbatim: true, detail: setup.cwd, selected: true, action: () => {} }] : []),
    { title: 'Browse other folders', icon: 'folder', divider: true, action: () => setBrowsing(true) },
  ]

  // Another agent starts from its own defaults: its model and modes are its own.
  const agentItems = (): MenuItem[] => agents.map((a) => ({
    title: a.name, verbatim: true, leading: <AgentIcon registry={store.agentRegistry(a.id)} size={16} />, selected: a.id === backend,
    action: () => (a.id === backend ? undefined : change({ backend: a.id, model: undefined, approval: undefined })),
  }))

  const approval = setup.approval ?? (bot ? { permission: bot.permission, mode: bot.mode } : PLAIN_APPROVAL)
  const { choices, selected } = approvals(approval, models.catalog)
  const ways = (): MenuItem[] => choices.map((c) => ({
    title: c.label, detail: c.note ?? undefined, selected: c.id === selected.id, action: () => change({ approval: withApproval(approval, c.id) }),
  }))

  const model = setup.model ?? (bot ? { id: bot.model ?? null, options: bot.modelOptions } : { id: null, options: {} })
  const { value, options, modelOptions } = modelChoices(models.catalog, model.id, tr)
  const chosen = (id: string, fallback?: string | null) => model.options[id] ?? fallback
  const modelLabel = [
    options.find((o) => o.id === value)?.label ?? value,
    ...modelOptions.map((o) => o.values.find((v) => v.id === chosen(o.id, o.default))?.name),
  ].filter(Boolean).join(' · ')
  const modelItems = (): MenuItem[] => [
    // Its settings belong to one model: another starts without any.
    ...options.map((o) => ({ title: o.label, verbatim: true, selected: o.id === value, action: () => (o.id === value ? undefined : change({ model: { id: o.id || null, options: {} } })) })),
    ...modelOptions.map((o, at) => ({
      title: o.name, verbatim: true, divider: at === 0,
      submenu: o.values.map((v) => ({ title: v.name, verbatim: true, selected: chosen(o.id, o.default) === v.id, action: () => change({ model: { id: model.id, options: { ...model.options, [o.id]: v.id } } }) })),
    })),
  ]

  const send = (text: string) => {
    setFailed('')
    // Without a bot, all it runs on goes with it: there is no bot's setup to fall back on.
    const asked = bot ? setup : { ...setup, backend, cwd: setup.cwd ?? home, approval }
    return store.startThread(bot?.id ?? null, text, asked).then(
      (s) => started({ computerId: of.ref.computerId, botId: s.botId }, s.taskId),
      (error: unknown) => {
        setFailed(error instanceof Error ? error.message : String(error))
        throw error
      },
    )
  }

  const draftKey = bot ? `${bot.id}/thread/new` : `${of.ref.computerId}/thread/new`
  return (
    <div className="chat-view thread-chat new-thread">
      <div className="conversation" ref={root}>
        <ChatLog
          items={[]}
          fresh={() => []}
          canLoadOlder={false}
          loadOlder={async () => {}}
          row={() => null}
          intro={
            <div className="intro-card">
              {bot ? <BotAvatar bot={bot} size={72} /> : <AgentIcon registry={store.agentRegistry(backend)} size={56} />}
              <Text as="h2" size="title2" weight="semibold" tone="text">
                {bot ? tr('New thread with {name}').replaceAll('{name}', () => bot.name) : tr('New thread with {name}').replaceAll('{name}', () => store.backendName(backend))}
              </Text>
            </div>
          }
        />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          <div className="thread-options">
            {/* Who it is with first; with one bot and no other way there is nothing to choose. */}
            <Option title="Choose a bot" items={others} disabled={bots.length + (plainOn ? 1 : 0) < 2} leading={bot ? <BotAvatar bot={bot} size={16} animated={false} /> : <Icon name="cpu" size={11} />} label={bot ? bot.name : tr('No bot')} strong />
            {bot ? null : <Option title="Agent" items={agentItems} disabled={agents.length < 2} leading={<AgentIcon registry={store.agentRegistry(backend)} size={12} />} label={store.backendName(backend)} />}
            <Option title="Workspace" items={folders} leading={<Icon name="folder" size={11} />} label={setup.cwd ? lastPathComponent(setup.cwd) : ownFolder} />
            <Option title="Permissions" items={ways} leading={<Icon name="hand.raised" size={11} />} label={tr(selected.label)} />
            <Option title="Model" items={modelItems} leading={<AgentIcon registry={store.agentRegistry(backend)} size={12} />} label={modelLabel} />
          </div>
          <Composer key={refKey(of.ref) + (bot ? '' : '/plain')} botId={bot?.id ?? ''} thread={{ draftKey, placeholder: tr('What should it work on?'), working: false, send }} />
          {failed ? <Text as="div" size="caption" tone="danger" className="thread-start-failed" role="alert">{failed}</Text> : null}
        </div>
      </div>
      <Sheet open={browsing} onClose={() => setBrowsing(false)} width={460} height={520}>
        <FolderPicker
          start={setup.cwd ?? (bot?.cwd || home || null)}
          onPick={(path) => {
            change({ cwd: path === (bot?.cwd ?? home) ? undefined : path })
            setBrowsing(false)
          }}
        />
      </Sheet>
    </div>
  )
}

/** One thing the thread runs on, as a chip that opens its choices. */
function Option({ title, items, leading, label, disabled = false, strong = false }: { title: string; items: () => MenuItem[]; leading: ReactNode; label: string; disabled?: boolean; strong?: boolean }) {
  const tr = useTr()
  return (
    <DropdownMenu items={items} title={tr(title)} className="thread-option" disabled={disabled}>
      {leading}
      <Text size="caption" tone={strong ? 'text' : 'secondary'} truncate>{label}</Text>
      {disabled ? null : <Icon name="chevron.down" size={8} color="var(--tertiary)" />}
    </DropdownMenu>
  )
}
