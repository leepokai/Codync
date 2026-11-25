import { useRef, useState } from 'react'
import { folderName, type Bot } from '@shared/models'
import type { ThreadPlace, ThreadSetup } from '@shared/work'
import { AgentIcon } from '../../components/AgentIcon'
import { BotAvatar, NoBotAvatar } from '../../components/Avatar'
import { Icon } from '../../components/Icon'
import type { MenuItem } from '../../components/Menu'
import { Sheet } from '../../components/Overlay'
import { Text } from '../../components/Text'
import { useTr } from '../../lib/use-language'
import { refKey, sameRef, type BotRef, type RosterItem } from '../../store/app-model'
import type { BotStore } from '../../store/bot-store'
import type { OutgoingFile } from '../../store/outgoing-files'
import { useRecentFolders } from '../../store/use-folder-picker'
import { useAgentModels } from '../bots/agent-models'
import { lastPathComponent } from '../bots/drafts'
import { FolderPicker } from '../bots/FolderPicker'
import { ChatLog, useDockHeight } from './ChatLog'
import { Composer } from './Composer'
import { ModelPicker } from '../bots/ModelPicker'
import { readySeats } from '../bots/seats'
import { Option, useApprovalMenu } from './RunOptions'
import { defaultFolder, modePlace, offersPlace, sentSetup, threadFolders } from './thread-folders'
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

/** Where in a git project a new thread can work, as its place menu lists them. */
const PLACES: { id: ThreadPlace; label: string }[] = [{ id: 'here', label: 'Here' }, { id: 'worktree', label: 'New worktree' }]

/** How a thread without a bot is approved until the user picks: it asks, in the agent's Auto mode, as a new bot. */
const PLAIN_APPROVAL = { permission: 'ask', mode: 'auto' }

/**
 * The page a thread starts from, in the form of a chat nothing was said in yet: the bot it is with in the middle, and
 * the chat's message box. Over the box, what the thread runs on, each a menu: the bot (or **No bot**, where the
 * computer can), which the others follow (they start as that bot's own), then the folder it works in, how it is
 * approved and its model with that model's settings, on the agent and login picked in the model panel's rail. In a git
 * project of the bot, where it works there: here or a new worktree. Without a bot the folder starts as a folder of
 * its own (an older host: the home folder) and nothing of Codync's goes with it. A choice is the thread's alone (`ThreadSetup`): the bot's
 * setup stays as it is. Sent, the thread is the one shown (`started`, with where its task is).
 * ponytail: the model menu lists every model without a search; the bot's settings have one if lists grow long.
 */
export function ThreadStart({ bots, computers = [], of, choose, started, base, folder, heading, placeholder }: {
  bots: RosterItem[]
  /** The connected computers a thread without a bot may start on, one item each: picked in the Computer menu. */
  computers?: RosterItem[]
  of: ThreadWith
  choose: (ref: BotRef, plain?: boolean) => void
  started: (ref: BotRef, taskId: string) => void
  /** What every thread started here carries besides the choices (a workflow it builds). */
  base?: ThreadSetup
  /** The folder it works in unless one is picked, when the computer chooses it (a workflow's own); its name. */
  folder?: string
  /** In the place of "New thread with …", and the box's prompt. */
  heading?: string
  placeholder?: string
}) {
  const tr = useTr()
  const { store } = of
  const bot = of.bot?.plain ? null : of.bot
  const [setup, setSetup] = useState<ThreadSetup>({})
  const change = (part: ThreadSetup) => setSetup((s) => ({ ...s, ...part }))
  const [failed, setFailed] = useState('')
  const [browsing, setBrowsing] = useState(false)
  const ready = (store.hello?.backends ?? []).filter((b) => b.available && b.signedIn !== false)
  const backend = setup.backend ?? bot?.backend ?? ready.find((b) => b.id === 'claude')?.id ?? ready[0]?.id ?? ''
  // Another agent than the bot's starts on its default login.
  const loginProfile = setup.loginProfile !== undefined ? setup.loginProfile || null : backend === bot?.backend ? bot.loginProfile ?? null : null
  const models = useAgentModels(backend, loginProfile)
  // A host that can't start a thread on a login is sent none: each agent stays on its default login, the bot's on its own.
  const logins = store.startsThreadsOnLogins
  const seats = readySeats({
    backends: store.hello?.backends ?? [], on: { backend, loginProfile }, tr, profiles: logins,
    pick: (b, id) => change({ backend: b.id, ...(logins ? { loginProfile: id ?? '' } : {}), ...(b.id === backend ? {} : { model: undefined, approval: undefined }) }),
  })
  const root = useRef<HTMLDivElement>(null)
  const dock = useRef<HTMLDivElement>(null)
  useDockHeight(root, dock)

  const many = new Set(bots.map((r) => r.ref.computerId)).size > 1
  const plainOn = store.startsThreadsWithoutBot
  const botItem = (r: RosterItem): MenuItem => ({
    title: r.bot.name, verbatim: true,
    leading: <BotAvatar bot={r.bot} size={22} animated={false} />, selected: bot !== null && sameRef(r.ref, of.ref), action: () => choose(r.ref),
  })
  // With several computers, the bots are grouped under their computer's name, in the roster's order.
  const byComputer = (): MenuItem[] => [...new Set(bots.map((r) => r.ref.computerId))].flatMap((id, at) => {
    const own = bots.filter((r) => r.ref.computerId === id)
    return [{ title: own[0].store.hostName, verbatim: true, icon: 'desktopcomputer', disabled: true, divider: at > 0, action: () => {} }, ...own.map(botItem)]
  })
  const others = (): MenuItem[] => [
    ...(many ? byComputer() : bots.map(botItem)),
    ...(plainOn ? [{ title: 'No bot', leading: <NoBotAvatar size={22} />, divider: bots.length > 0, selected: bot === null, action: () => choose(of.ref, true) }] : []),
  ]
  // Without a bot, the computer it runs on is its own choice.
  const computerItems = (): MenuItem[] => computers.map((r) => ({
    title: r.store.hostName, verbatim: true, icon: 'desktopcomputer', selected: r.ref.computerId === of.ref.computerId,
    action: () => choose({ computerId: r.ref.computerId, botId: '' }, true),
  }))

  const home = store.hello?.home ?? ''
  // A thread without a bot gets a folder of its own from a host that makes one.
  const ownFolders = !bot && !folder && store.givesThreadsOwnFolders
  const ownFolder = folder ?? (bot ? folderName(bot, tr) : ownFolders ? tr('Own folder') : lastPathComponent(home) || home)
  // From a host with projects: the bot's projects first, then its own folder, then recent folders.
  const listsProjects = !!bot && !folder && store.hasProjects
  const recent = useRecentFolders(listsProjects)
  const projectFolders = (b: Bot): MenuItem[] => {
    const list = threadFolders(b, recent)
    const picked = setup.cwd ?? defaultFolder(b)
    return [
      ...list.map((f, at): MenuItem => ({
        title: f.home ? 'Own folder' : lastPathComponent(f.path), verbatim: !f.home, detail: f.path,
        divider: f.recent && !list[at - 1]?.recent, selected: picked === f.path, action: () => change({ cwd: f.cwd }),
      })),
      // One picked with Browse that isn't listed.
      ...(list.some((f) => f.path === picked) ? [] : [{ title: lastPathComponent(picked), verbatim: true, detail: picked, selected: true, action: () => {} }]),
      { title: 'Browse other folders', icon: 'folder', divider: true, action: () => setBrowsing(true) },
    ]
  }
  const folders = (): MenuItem[] => listsProjects && bot ? projectFolders(bot) : [
    { title: ownFolder, verbatim: true, detail: folder || bot?.managedWorkspace || ownFolders ? undefined : (bot?.cwd ?? home), selected: !setup.cwd, action: () => change({ cwd: undefined }) },
    ...(setup.cwd ? [{ title: lastPathComponent(setup.cwd), verbatim: true, detail: setup.cwd, selected: true, action: () => {} }] : []),
    { title: 'Browse other folders', icon: 'folder', divider: true, action: () => setBrowsing(true) },
  ]

  // On another agent than its bot's, a thread starts from that agent's own defaults.
  const own = bot && backend === bot.backend ? bot : null
  const approval = setup.approval ?? (own ? { permission: own.permission, mode: own.mode } : PLAIN_APPROVAL)
  const ways = useApprovalMenu(approval, models, (a) => change({ approval: { permission: a.permission, mode: a.mode ?? null } }))
  const model = setup.model ?? (own ? { id: own.model ?? null, options: own.modelOptions } : { id: null, options: {} })

  // In a git project of the bot, where there: the bot's worktree mode until the user picks (none shown for side by side).
  const placeOn = !!bot && !folder && store.startsThreadsInPlace && offersPlace(bot, setup.cwd ?? defaultFolder(bot))
  const place = setup.place ?? modePlace(bot?.worktreeMode ?? null)
  const places = (): MenuItem[] => PLACES.map((p) => ({ title: p.label, selected: p.id === place, action: () => change({ place: p.id }) }))

  const send = (text: string, files: OutgoingFile[]) => {
    setFailed('')
    // Without a bot, all it runs on goes with it: there is no bot's setup to fall back on.
    const asked = sentSetup(
      { ...base, ...(own ? setup : bot ? { ...setup, model, approval } : { ...setup, backend, approval }) },
      { offered: placeOn, plain: !bot, home, hostChooses: !!folder || ownFolders },
    )
    return store.startThread(bot?.id ?? null, text, asked, files).then(
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
                {heading ?? tr('New thread with {name}').replaceAll('{name}', () => bot ? bot.name : store.backendName(backend))}
              </Text>
            </div>
          }
        />
        <div className="composer-dock" ref={dock}>
          <div className="composer-fade" />
          {/* What it runs on, in the box's row as in a bot's chat: who it is with first (with one bot and no other way
              there is nothing to choose). */}
          <Composer key={refKey(of.ref) + (bot ? '' : '/plain')} botId={bot?.id ?? ''} thread={{ draftKey, placeholder: placeholder ?? tr('What should it work on?'), working: false, files: store.startsThreadsWithFiles, keepFiles: `thread/new/${base?.workflow ?? ''}`, send, controls: <>
            <Option title="Choose a bot" items={others} search={bots.length > 8 ? 'Search bots' : undefined} disabled={bots.length + (plainOn ? 1 : 0) < 2} leading={bot ? <BotAvatar bot={bot} size={16} animated={false} /> : <NoBotAvatar size={16} />} label={bot ? bot.name : tr('No bot')} strong />
            {bot ? null : <Option title="Computer" items={computerItems} disabled={computers.length < 2} leading={<Icon name="desktopcomputer" size={11} />} label={store.hostName} />}
            <Option title="Workspace" items={folders} leading={<Icon name="folder" size={11} />} label={setup.cwd ? (setup.cwd === bot?.cwd ? tr('Own folder') : lastPathComponent(setup.cwd)) : ownFolder} />
            {placeOn ? <Option title="Worktree mode" items={places} leading={<Icon name="arrow.triangle.branch" size={11} />} label={tr(PLACES.find((p) => p.id === place)?.label ?? 'Side by side')} /> : null}
            {/* The agent and its login are in the panel's rail; another agent starts from its own defaults. */}
            <ModelPicker
              backend={backend} model={model} models={models} set={(m) => change({ model: m })} approval={ways} seats={seats}
            />
          </> }} />
          {failed ? <Text as="div" size="caption" tone="danger" className="thread-start-failed" role="alert">{failed}</Text> : null}
        </div>
      </div>
      <Sheet open={browsing} onClose={() => setBrowsing(false)} width={460} height={520}>
        <FolderPicker
          start={setup.cwd ?? ((bot ? defaultFolder(bot) : home) || null)}
          onPick={(path) => {
            change({ cwd: !ownFolders && path === (bot ? defaultFolder(bot) : home) ? undefined : path })
            setBrowsing(false)
          }}
        />
      </Sheet>
    </div>
  )
}
