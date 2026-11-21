import { useRef, useState, type ReactNode } from 'react'
import { approvals, withApproval } from '@shared/approvals'
import { folderName } from '@shared/models'
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
import { modelChoices, useAgentModels } from '../bots/agent-models'
import { lastPathComponent } from '../bots/drafts'
import { FolderPicker } from '../bots/FolderPicker'
import { ChatLog, useDockHeight } from './ChatLog'
import { Composer } from './Composer'
import './thread.css'
import './composer.css'
import './chat-controls.css'
import './thread-chat.css'

/**
 * The page a thread starts from, in the form of a chat nothing was said in yet: the bot it is with in the middle, and
 * the chat's message box. Over the box, what the thread runs on, each a menu: the bot, which the others follow (they
 * start as that bot's own), then the folder it works in, how it is approved and its model with that model's settings.
 * A choice is the thread's alone (`ThreadSetup`): the bot's setup stays as it is. Sent, the thread is the one shown
 * (`started`).
 * ponytail: the model menu lists every model without a search; the bot's settings have one if lists grow long.
 */
export function ThreadStart({ bots, of, choose, started }: { bots: RosterItem[]; of: RosterItem; choose: (ref: BotRef) => void; started: (itemId: string) => void }) {
  const tr = useTr()
  const { bot, store } = of
  const [setup, setSetup] = useState<ThreadSetup>({})
  const change = (part: ThreadSetup) => setSetup((s) => ({ ...s, ...part }))
  const [failed, setFailed] = useState('')
  const [browsing, setBrowsing] = useState(false)
  const models = useAgentModels(bot.backend)
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)

  const many = new Set(bots.map((r) => r.ref.computerId)).size > 1
  const others = (): MenuItem[] => bots.map((r) => ({
    title: r.bot.name, verbatim: true, detail: many ? r.store.hostName : undefined,
    leading: <BotAvatar bot={r.bot} size={22} animated={false} />, selected: sameRef(r.ref, of.ref), action: () => choose(r.ref),
  }))

  const folders = (): MenuItem[] => [
    { title: folderName(bot, tr), verbatim: true, detail: bot.managedWorkspace ? undefined : bot.cwd, selected: !setup.cwd, action: () => change({ cwd: undefined }) },
    ...(setup.cwd ? [{ title: lastPathComponent(setup.cwd), verbatim: true, detail: setup.cwd, selected: true, action: () => {} }] : []),
    { title: 'Browse other folders', icon: 'folder', divider: true, action: () => setBrowsing(true) },
  ]

  const approval = setup.approval ?? { permission: bot.permission, mode: bot.mode }
  const { choices, selected } = approvals(approval, models.catalog)
  const ways = (): MenuItem[] => choices.map((c) => ({
    title: c.label, detail: c.note ?? undefined, selected: c.id === selected.id, action: () => change({ approval: withApproval(approval, c.id) }),
  }))

  const model = setup.model ?? { id: bot.model ?? null, options: bot.modelOptions }
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
    return store.startThread(bot.id, text, setup).then(started, (error: unknown) => {
      setFailed(error instanceof Error ? error.message : String(error))
      throw error
    })
  }

  return (
    <div className="thread-view thread-chat new-thread">
      <div className="conversation" ref={root}>
        <ChatLog
          items={[]}
          fresh={() => []}
          canLoadOlder={false}
          loadOlder={async () => {}}
          row={() => null}
          intro={
            <div className="intro-card">
              <BotAvatar bot={bot} size={72} />
              <Text as="h2" size="title2" weight="semibold" tone="text">{tr('New thread with {name}').replaceAll('{name}', () => bot.name)}</Text>
            </div>
          }
        />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          <div className="thread-options">
            {/* Who it is with first; on one bot there is nothing to choose. */}
            <Option title="Choose a bot" items={others} disabled={bots.length < 2} leading={<BotAvatar bot={bot} size={16} animated={false} />} label={bot.name} strong />
            <Option title="Workspace" items={folders} leading={<Icon name="folder" size={11} />} label={setup.cwd ? lastPathComponent(setup.cwd) : folderName(bot, tr)} />
            <Option title="Permissions" items={ways} leading={<Icon name="hand.raised" size={11} />} label={tr(selected.label)} />
            <Option title="Model" items={modelItems} leading={<AgentIcon registry={store.agentRegistry(bot.backend)} size={12} />} label={modelLabel} />
          </div>
          <Composer key={refKey(of.ref)} botId={bot.id} thread={{ draftKey: `${bot.id}/thread/new`, placeholder: tr('What should it work on?'), working: false, send }} />
          {failed ? <Text as="div" size="caption" tone="danger" className="thread-start-failed" role="alert">{failed}</Text> : null}
        </div>
      </div>
      <Sheet open={browsing} onClose={() => setBrowsing(false)} width={460} height={520}>
        <FolderPicker
          start={setup.cwd ?? (bot.cwd || store.hello?.home || null)}
          onPick={(path) => {
            change({ cwd: path === bot.cwd ? undefined : path })
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
