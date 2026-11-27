import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import { useListFlip } from '../lib/slide'
import { alertAnalyticsKind } from '@shared/alerts'
import type { WindowCommand } from '@shared/ipc'
import { isSSHConnecting, showsChat as shouldShowChat } from '@shared/ssh-startup'
import { detailMinWidth, frameInset, railWidth, sidebarMinWidth } from '@shared/chat-layout'
import { draftOf, isGroup, type Bot, type BotDraft } from '@shared/models'
import { isLive, threadSections } from '@shared/work'
import { CharacterAvatar } from '../components/Avatar'
import { Button, IconButton, ResizeHandle } from '../components/Controls'
import { Dialog, ModalHeader, Sheet } from '../components/Overlay'
import { AnchoredMenu, type MenuItem } from '../components/Menu'
import { Icon } from '../components/Icon'
import { HStack, Spacer } from '../components/Stack'
import { noVisits, step, visit } from '../lib/visits'
import { NO_BAR_SLOTS, WindowBar, WindowBarSlots } from './WindowBar'
import { Text } from '../components/Text'
import { useModels } from '../lib/observable'
import { prefs, stepTextSize, usePref, DEFAULT_TEXT_SIZE } from '../lib/prefs'
import { limitSectionName, sectionName } from '../lib/roster-sections'
import { useApp, StoreContext } from '../store/context'
import { analytics } from '../store/analytics'
import { refKey, sameRef, parseRef, type AppModel, type BotRef } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import { useComputerRoster } from '../store/computer-roster'
import { withOlderThreads } from '../store/older-threads'
import { BotRoster } from './BotRoster'
import { NavRail, OpenExtensions, pageIcon, type AppPage } from './NavRail'
import { ThreadDetail, ThreadList, canStartThread, type ThreadPick } from './ThreadsPage'
import { botItems, threadItems, workflowItems } from './search-items'
import { ComputerFilterHeader, computerSelection } from './ComputerFilterHeader'
import { ChatView } from './chat/ChatView'
import { queueVoiceKey } from './chat/voice-shortcut-keys'
import { NewBotView, NewGroupView } from './bots/NewBotView'
import { ImportBotsSheet } from './bots/ImportBotsView'
import { BotEditorView } from './bots/BotEditorView'
import type { ExtensionFocus } from './marketplace/market-models'
import { MarketplaceView } from './marketplace/MarketplaceView'
import { ScreenViewer } from './screen/ScreenViewer'
import { SignInCodeDialog } from './marketplace/ConnectorSheets'
import { SettingsView, type SettingsPage } from './settings/SettingsView'
import { ApprovalSheet } from './settings/ApprovalSheet'
import { ResetAllDataDialogs } from './settings/ResetSection'
import { FindBar } from './FindBar'
import { useFindBar } from './use-conversation-find'
import { SearchPalette } from './SearchPalette'
import { WorkflowsView, type WorkflowFocus } from './workflows/WorkflowsView'
import { WorkflowList, type WorkflowPlace } from './workflows/WorkflowList'
import { computerSymbol } from './ComputerBadge'
import { canOpenWorkflows } from './workflows/pins'
import { computerChoice, firstAmong } from './computer-choice'
import { extensionsTarget, railExtensionsOf } from './extensions-target'
import { AccountWelcomeView, useWelcomeGate } from './AccountWelcomeView'
import { ComputerAccessView } from './computer-access/ComputerAccessView'
import { AccessOnboarding } from './computer-access/AccessOnboarding'
import { HostStateView } from './HostStateView'
import { StartupRecovery } from './StartupRecovery'
export { EmptyState } from './HostStateView'
import { RefreshIndicator } from './RefreshIndicator'
import { MainPrompts } from './MainPrompts'
import { PairingRequestPrompt } from './PairingRequestPrompt'
import { OpenVoiceSettings } from './call/CallView'
import { DesktopActionMenu } from './DesktopActionMenu'
import { moveToSectionItems, sectionHeaderItems } from './section-menus'
import { makeLeadItems, offersDelete, voiceTarget } from './lead-menus'
import { NameDialog, type Naming } from './NameDialog'
import { useTraySummary } from './tray-summary'
import { useIslandSummary } from './island-summary'
import { useSSH, useSSHAttachments } from './settings/ssh-model'
import { useCloudAttachments } from '../store/cloud-attachments'
import './chat-window.css'
import './sidebar-header.css'
import { useTr } from '../lib/use-language'


/** Opens the bot the tray or a notification names (`computerId/botId`). */
function openRef(app: AppModel, key: string) {
  const ref = parseRef(key)
  if (ref) app.select(ref)
}

/** The chat window: the rail of its pages on the left, then the page (Bots: the roster and the conversation). */
/** The workflow a clicked notification opens, or null when it opens its bot's chat. */
function alertWorkflow(c: Extract<WindowCommand, { kind: 'openAlert' }>) {
  const ref = parseRef(c.ref)
  return ref && c.workflowId ? { computerId: ref.computerId, workflowId: c.workflowId } : null
}

export function ChatWindow() {
  const tr = useTr()
  const app = useApp()
  const welcome = useWelcomeGate()
  const [accessSetup, setAccessSetup] = usePref(prefs.computerAccessSetup)
  const [showAccess, setShowAccess] = useState(false)
  const [confirmReset, setConfirmReset] = useState(false)
  const [upToDate, setUpToDate] = useState(false)
  useSSHAttachments(app)
  useCloudAttachments(app)
  const ssh = useSSH()
  const showsChat = shouldShowChat(app.host.state.kind, app.computers.length, ssh.state)
  const finishAccess = (ready: boolean, onboarding: boolean) => {
    setAccessSetup(ready ? 'ready' : 'later')
    analytics.track({ event: 'computer_access_finished', ready, onboarding })
  }

  useEffect(
    () =>
      window.codync.app.onCommand((c: WindowCommand) => {
        switch (c.kind) {
          case 'computerAccess':
            return setShowAccess(true)
          case 'confirmReset':
            return setConfirmReset(true)
          case 'upToDate':
            return setUpToDate(true)
          case 'reviewApprovals':
            return app.reviewApprovals()
          case 'setTextSize':
            return prefs.textSize.set(c.size)
          case 'stepTextSize':
            return prefs.textSize.set(c.up === null ? DEFAULT_TEXT_SIZE : stepTextSize(prefs.textSize.get(), c.up))
          case 'setUsageIconStyle':
            return prefs.usageIconStyle.set(c.style)
          case 'openBot':
            return openRef(app, c.ref)
          case 'openAlert':
            analytics.track({ event: 'desktop_notification_opened', kind: alertAnalyticsKind(c.alert) })
            // A workflow's alert is opened on its page (ChatSplitView); any other, in its bot's chat.
            return alertWorkflow(c) ? undefined : openRef(app, c.ref)
          case 'stopBot': {
            const ref = parseRef(c.ref)
            const store = ref && app.store(ref.computerId)
            if (!ref || !store) return
            // The tray's one Stop is for all the bot does: its chat's turn and its work in the background.
            store.stop(ref.botId)
            if (store.bots.get(ref.botId)?.work.some(isLive)) store.stopWork(ref.botId)
            return
          }
          case 'refresh':
            return void app.refresh()
          case 'setRemoteScreen':
            void app.setRemoteScreen(c.on)
            return
          case 'respondPermission': {
            const ref = parseRef(c.ref)
            const store = ref ? app.store(ref.computerId) : null
            const entry = ref ? store?.allEntries(ref.botId).find((e) => e.id === c.entryId) : undefined
            if (store && entry) store.respond(entry, c.optionId)
            return
          }
          case 'sendMessage': {
            const ref = parseRef(c.ref)
            if (ref) app.store(ref.computerId)?.send(c.text, ref.botId)
            return
          }
        }
      }),
    [app],
  )

  const approval = app.currentApproval
  return (
    <div className="chat-window">
      <Summaries />
      <RefreshIndicator />
      {!welcome.shown ? (
        accessSetup === null ? (
          <AccessOnboarding onContinue={(ready) => finishAccess(ready, true)} onBack={welcome.back} />
        ) : showsChat ? (
          <ChatSplitView />
        ) : (
          <HostStateView />
        )
      ) : (
        <AccountWelcomeView onContinue={welcome.finish} />
      )}
      {!welcome.shown && accessSetup !== null && showsChat ? null : <div className="window-drag drag" />}
      <MainPrompts ready={!welcome.shown && accessSetup !== null} />
      <Sheet open={showAccess} onClose={() => setShowAccess(false)} width={660} height={Math.min(820, window.innerHeight - 80)}>
        <ModalHeader icon="hand.raised" title={tr(`Computer access`)} />
        <ComputerAccessView onContinue={(ready) => { finishAccess(ready, false); setShowAccess(false) }} />
      </Sheet>
      <ResetAllDataDialogs open={confirmReset} onClose={() => setConfirmReset(false)} />
      <Dialog
        open={upToDate}
        title={tr("You're up to date")}
        message={tr('Codync {version} is currently the newest version available.').replaceAll('{version}', window.codync.appVersion)}
        actions={[]}
        cancel="OK"
        onClose={() => setUpToDate(false)}
      />
      {/* Approving a device: the code the device shows must match (spec §4.2 B). Closing puts it off. */}
      <Sheet open={approval !== null} onClose={() => approval && app.deferApproval(approval)}>
        {approval ? <ApprovalSheet key={approval.id} approval={approval} /> : null}
      </Sheet>
      <PairingRequestPrompt />
    </div>
  )
}

/** Keeps the tray and the approval island current: it follows every store, so it renders nothing. */
function Summaries() {
  const app = useApp()
  useTraySummary(app)
  useIslandSummary(app)
  return null
}

interface BotTarget {
  bot: Bot
  store: BotStore
}

interface EditTarget {
  id: string
  draft: BotDraft
  store: BotStore
}

/** A new bot or group chat being made, and the computer it goes to. */
interface CreateTarget {
  kind: 'bot' | 'group'
  computerId: string
}

function ChatSplitView() {
  const tr = useTr()
  const app = useApp()
  const ssh = useSSH()
  const connecting = ssh.state.profiles.some((p) => isSSHConnecting(ssh.status(p.id))) || [...app.stores.values()].some((s) => s.shownConnection.kind === 'connecting')
  const [listHidden, setListHidden] = usePref(prefs.listHidden)
  const [botsByComputer, setBotsByComputer] = usePref(prefs.botsByComputer)
  const [botsByStatus, setBotsByStatus] = usePref(prefs.botsByStatus)
  const [threadsGroup, setThreadsGroup] = usePref(prefs.threadsGroup)
  const [threadsByStatus, setThreadsByStatus] = usePref(prefs.threadsByStatus)
  const [threadsByActivity, setThreadsByActivity] = usePref(prefs.threadsByActivity)
  const [sidebarWidth, setSidebarWidth] = usePref(prefs.sidebarWidth)
  const [hidden, setHidden] = usePref(prefs.hiddenComputers)
  const [searching, setSearching] = useState(false)
  // Find in the open conversation (⌘F).
  const find = useFindBar(app.selection ? refKey(app.selection) : null)
  const [creating, setCreating] = useState<CreateTarget | null>(null)
  const [importing, setImporting] = useState(false)
  const [newMenu, setNewMenu] = useState(false)
  const [context, setContext] = useState<{ items: MenuItem[]; x: number; y: number } | null>(null)
  const [naming, setNaming] = useState<Naming | null>(null)
  const [editing, setEditing] = useState<EditTarget | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<BotTarget | null>(null)
  const [newSessionBot, setNewSessionBot] = useState<BotTarget | null>(null)
  const [page, setPage] = useState<AppPage>('bots')
  // The Settings entry's page. Computers, Network and Usage have entries of their own.
  const [settingsPage, setSettingsPage] = useState<SettingsPage>('general')
  const openVoiceSettings = useCallback(() => {
    setSettingsPage('voice')
    setPage('settings')
  }, [])
  // The computer whose Extensions show, once one was asked for by name (its heading in the roster, the page's picker).
  const [extensionsOf, setExtensionsOf] = useState<string | null>(null)
  // The computer whose workflows show, once one was asked for by name (its heading or a pin in the roster), and the workflow to open there.
  const [workflowsOf, setWorkflowsOf] = useState<string | null>(null)
  const [workflowFocus, setWorkflowFocus] = useState<WorkflowFocus | null>(null)
  // The workflow on the canvas, for the list beside it and to show again when the page comes back.
  const [shownWorkflow, setShownWorkflow] = useState<WorkflowPlace | null>(null)
  // The thread the Threads page shows.
  const [picked, setPicked] = useState<ThreadPick | null>(null)
  // The bot a new thread starts with, where the user chose one; the page it starts from shows with no thread picked.
  const [starting, setStarting] = useState<BotRef | null>(null)
  const startThread = (ref: BotRef | null) => {
    setStarting(ref)
    setPicked(null)
  }
  const [windowSize, setWindowSize] = useState({ width: window.innerWidth, height: window.innerHeight })
  const newButton = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  const sidebar = useRef<HTMLElement>(null)
  useModels([...app.stores.values()])

  useEffect(() => {
    const onResize = () => {
      setWindowSize({ width: window.innerWidth, height: window.innerHeight })
      setSidebarWidth(Math.min(prefs.sidebarWidth.get(), Math.max(sidebarMinWidth, window.innerWidth - railWidth - frameInset - detailMinWidth)))
    }
    // A window opened narrow starts with a list that fits it.
    onResize()
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [setSidebarWidth])

  const sheetHeight = Math.max(420, windowSize.height - 76)
  const stores = app.computers.map((c) => app.store(c.id)).filter((s): s is BotStore => !!s)
  const shown = new Set(computerSelection(app.computers.map((c) => c.id), hidden).shown)
  const shownStores = stores.filter((s) => shown.has(s.computer.id))
  const onlineStores = shownStores.filter((s) => s.connection.kind === 'online' && !s.mismatch)
  const selectedStore = app.selectedStore
  const screenStore = app.screenComputer ? (app.store(app.screenComputer) ?? null) : null
  const createStore = creating ? app.store(creating.computerId) : null
  const localOnline = !!app.local && onlineStores.includes(app.local)
  const roster = useComputerRoster(app, shown)
  const visibleRoster = roster.items
  // The Threads page lists the boxes of threads without a bot too, of the computers shown, and their older threads.
  const threadRoster = withOlderThreads([...roster.ordered, ...app.plainBoxes.filter((r) => shown.has(r.ref.computerId))], (of) => of.store.olderThreads.shown)
  const threads = threadSections(threadRoster, threadsGroup, (of) => refKey(of.ref), (of) => of.store.hostName, { status: threadsByStatus, active: threadsByActivity })
  /** A bot's chat, on the Bots page. */
  const openBot = (ref: BotRef) => {
    app.select(ref)
    setPage('bots')
  }
  // Back and Forward: the pages shown, and on Bots each bot.
  const selectionKey = app.selection ? refKey(app.selection) : null
  type Place = { page: AppPage; ref: BotRef | null }
  const [visits, setVisits] = useState(noVisits<Place>)
  useEffect(() => {
    const place: Place = { page, ref: page === 'bots' ? app.selection : null }
    setVisits((v) => visit(v, place, (a, b) => a.page === b.page && (a.ref && refKey(a.ref)) === (b.ref && refKey(b.ref))))
  }, [page, selectionKey])
  const travel = (by: -1 | 1) => {
    const next = step(visits, by)
    const place = next?.places[next.at]
    if (!next || !place) return
    setVisits(next)
    setPage(place.page)
    // A bot deleted since is passed over: the page shows what is selected now.
    if (place.ref && app.store(place.ref.computerId)?.bots.has(place.ref.botId)) app.select(place.ref)
  }
  const [barTitle, setBarTitle] = useState<HTMLElement | null>(null)
  const [barEnd, setBarEnd] = useState<HTMLElement | null>(null)
  /** The rail: another page, or, on the entry of the page shown, its list folded away and brought back. */
  const go = (next: AppPage) => {
    // The rail's Extensions follows what is selected now: a computer asked for earlier (a heading, a deep link)
    // must not keep the page on that computer after the user picked another bot.
    setExtensionsOf((asked) => railExtensionsOf(asked, next))
    if (next !== page) setPage(next)
    else if (listed) setListHidden(!listHidden)
  }
  /** The Workflows list unfolds a computer whose workflow the canvas turns to, so its row shows. */
  const unfoldWorkflows = (computerId: string) => {
    const folded = prefs.foldedWorkflowComputers.get()
    if (folded.includes(computerId)) prefs.foldedWorkflowComputers.set(folded.filter((id) => id !== computerId))
  }
  /** A new workflow on `computerId`, shown once made. */
  const newWorkflow = (computerId: string) => {
    unfoldWorkflows(computerId)
    setWorkflowsOf(computerId)
    setWorkflowFocus({ create: true })
  }
  const openWorkflows = (computerId: string, workflowId?: string) => {
    unfoldWorkflows(computerId)
    setWorkflowsOf(computerId)
    setWorkflowFocus(workflowId ? { id: workflowId } : null)
    setPage('workflows')
  }
  // A new object each time, so opening the same tab again still turns Extensions to it.
  const [extensionsFocus, setExtensionsFocus] = useState<ExtensionFocus | null>(null)
  const openExtensions = (computerId: string, focus?: ExtensionFocus) => {
    setExtensionsOf(computerId)
    setExtensionsFocus(focus ? { ...focus } : null)
    setPage('extensions')
  }

  const create = (kind: CreateTarget['kind']) => {
    // The selected bot's computer if it's online, else this computer, else any online one.
    const target = firstAmong(onlineStores, [selectedStore, app.local])
    if (target) setCreating({ kind, computerId: target.computer.id })
  }
  const newChoice = creating ? computerChoice(onlineStores, creating.computerId, (computerId) => setCreating({ ...creating, computerId })) : null

  useEffect(
    () =>
      window.codync.app.onCommand((c) => {
        if (c.kind === 'newBot' && onlineStores.length) create('bot')
        if (c.kind === 'search') setSearching(true)
        // Without a conversation in sight there is nothing to find in: ⌘F searches the bots instead.
        if (c.kind === 'find') page === 'bots' && app.selection ? find.open() : setSearching(true)
        if (c.kind === 'networkSettings') setPage('network')
        if (c.kind === 'toggleSidebar') setListHidden(!prefs.listHidden.get())
        // The window's own handler selects the bot; its chat is on the Bots page.
        if (c.kind === 'openBot') setPage('bots')
        if (c.kind === 'openAlert') {
          const workflow = alertWorkflow(c)
          if (workflow) openWorkflows(workflow.computerId, workflow.workflowId)
          else setPage('bots')
        }
        if (c.kind === 'voiceKey') {
          if (c.key === 'double' && !app.selection) app.select(voiceTarget(roster)?.ref ?? null)
          setPage('bots')
          queueVoiceKey(c.key)
        }
      }),
  )

  // Screenshot/UI checks (development builds): open a screen at launch.
  const debugOpened = useRef(false)
  useEffect(() => {
    const target = window.codync.debugOpen
    if (!target || debugOpened.current) return
    if (target === 'newBot' || target === 'newGroup') {
      if (!onlineStores.length) return
      debugOpened.current = true
      create(target === 'newBot' ? 'bot' : 'group')
    } else if (target === 'importBots') {
      if (!localOnline) return
      debugOpened.current = true
      setImporting(true)
    } else if (target === 'plugins') {
      if (!app.local) return
      debugOpened.current = true
      openExtensions(app.local.computer.id)
    } else if (target === 'computers') {
      debugOpened.current = true
      setPage('computers')
    } else {
      const item = app.roster.find((i) => i.bot.name === target)
      if (!item) return
      debugOpened.current = true
      app.select(item.ref)
    }
  })

  const newItems = (): MenuItem[] => [
    { title: 'New bot', icon: 'plus', action: () => create('bot') },
    { title: 'New group chat', icon: 'person.2', action: () => create('group') },
    ...(localOnline ? [{ title: 'Import bots…', icon: 'square.and.arrow.down', action: () => setImporting(true) }] : []),
  ]

  // Of the computers offering workflows: the one asked for, else the selected bot's, this one, any.
  const workflowStores = onlineStores.filter(canOpenWorkflows)
  const workflowStore = firstAmong(workflowStores, [workflowsOf ? app.store(workflowsOf) : null, selectedStore, app.local])
  // The header's + on Workflows: the computers listed that take a new workflow, picked from a menu when there are several.
  const workflowHomes = shownStores.filter(canOpenWorkflows)
  const newWorkflowItems = (): MenuItem[] => workflowHomes.map((s) => ({ title: s.hostName, verbatim: true, icon: computerSymbol(s.computer.device), action: () => newWorkflow(s.computer.id) }))
  const extensionsComputer = extensionsTarget(
    extensionsOf,
    selectedStore?.computer.id,
    app.local?.computer.id,
    onlineStores[0]?.computer.id,
  )
  const extensionsStore = (extensionsComputer ? app.store(extensionsComputer) : null) ?? selectedStore ?? app.local ?? onlineStores[0] ?? null
  const extensionsChoice = computerChoice(onlineStores, extensionsStore?.computer.id, setExtensionsOf)

  // One plain list. The lead stays in sight: no hide.
  const botActions = (bot: Bot, store: BotStore): MenuItem[] => [
    { title: bot.pinned ? 'Unpin' : 'Pin', icon: 'pin', action: () => store.setPinned(bot, !bot.pinned) },
    ...moveToSectionItems(bot, store, setNaming),
    { title: 'Mark as Read', icon: 'bell.badge', action: () => store.markAllRead(bot.id) },
    { title: 'Edit Profile…', icon: 'square.and.pencil', action: () => setEditing({ id: crypto.randomUUID(), draft: draftOf(bot), store }) },
    { title: 'Copy conversation ID', icon: 'square.on.square', action: () => window.codync.app.copy(bot.id) },
    // A group has no agent session of its own.
    ...(isGroup(bot) ? [] : [{ title: 'New session', icon: 'arrow.counterclockwise', action: () => setNewSessionBot({ bot, store }) }]),
    ...makeLeadItems(bot, store),
    ...(bot.lead ? [] : [{ title: 'Hide from sidebar', icon: 'eye.slash', action: () => store.setHidden(bot, true) }]),
    ...(offersDelete(bot, store) ? [{ title: 'Delete', icon: 'trash', destructive: true, action: () => setConfirmDelete({ bot, store }) }] : []),
  ]

  // ↑/↓ in the roster move the selection; Alt+↑/↓ moves the selected bot itself.
  const onListKey = (e: React.KeyboardEvent) => {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
    if (e.target instanceof HTMLElement && e.target.closest('.computer-roster-heading')) return
    if (e.altKey) {
      e.preventDefault()
      if (app.selection) roster.moveBot(app.selection, e.key === 'ArrowUp' ? -1 : 1)
      return
    }
    const refs = roster.navigable.map((i) => i.ref)
    if (!refs.length) return
    e.preventDefault()
    const current = refs.findIndex((r) => sameRef(r, app.selection))
    const next = e.key === 'ArrowDown' ? Math.min(current + 1, refs.length - 1) : Math.max(current - 1, 0)
    app.select(refs[next]!)
  }

  // The list's edge drags its width; dragged most of the way shut, it folds away.
  const [resizing, setResizing] = useState(false)
  const sidebarMax = Math.min(420, window.innerWidth - railWidth - frameInset - detailMinWidth)
  const resizeSidebar = (proposed: number) => {
    const hide = proposed < 120
    if (hide !== listHidden) setListHidden(hide)
    if (!hide) setSidebarWidth(Math.min(Math.max(sidebarMinWidth, proposed), sidebarMax))
  }

  const errorMessage = app.errorMessage
  const mismatchStores = shownStores.filter((s) => s.mismatch)
  const selected = app.selection
  const selectedBot = selected && selectedStore?.bots.get(selected.botId)

  let detail: ReactNode
  if (selected && selectedStore && selectedBot) {
    detail = (
      <StoreContext.Provider value={selectedStore}>
        <ChatView key={refKey(selected)} botId={selected.botId} />
      </StoreContext.Provider>
    )
  } else if (!onlineStores.length && ssh.state.profiles.length) {
    detail = <StartupRecovery manage={() => setPage('computers')} />
  } else {
    detail = (
      <div className="empty-detail">
        <div className="detail-drag drag" />
        <HStack align="stretch" gap={12}>
          <CharacterAvatar shape="blob" color="blue" size={60} mood="working" />
          <CharacterAvatar shape="squircle" color="orange" size={60} />
          <CharacterAvatar shape="teardrop" color="violet" size={60} mood="working" />
        </HStack>
        <Text as="div" size="title2" weight="semibold">{connecting ? tr(`Connecting to your computers…`) : tr(`Your coding agents, as teammates.`)}</Text>
        <Text as="div" tone="secondary" className="empty-detail-message">{connecting ? tr(`Your bots will appear once connected. Manage computers to see connection progress.`) : tr(`Pick a bot, or create one for each kind of work and point it at a project.`)}</Text>
        <Button onClick={() => create('bot')} disabled={!onlineStores.length}>
          New bot
        </Button>
      </div>
    )
  }

  const width = listHidden ? 0 : sidebarWidth
  // The pages with a list beside their content: its title, its header's + (and the menu that + opens), and what its Search finds.
  const lists = {
    bots: {
      title: 'Bots',
      add: <IconButton title="New" icon="plus" size={32} className="sidebar-header-action" buttonRef={newButton} menuOpen={newMenu} onClick={() => setNewMenu((o) => !o)} disabled={!onlineStores.length} />,
      menu: newItems,
      search: { label: 'Search bots', empty: 'No matching bots', items: () => botItems(visibleRoster, openBot) },
    },
    threads: {
      title: 'Threads',
      add: <IconButton title="New thread" icon="plus" size={32} className="sidebar-header-action" onClick={() => startThread(null)} disabled={!canStartThread(threadRoster)} />,
      menu: undefined,
      search: { label: 'Search', empty: 'No results', items: () => threadItems(threadRoster, setPicked) },
    },
    workflows: {
      title: 'Workflows',
      add: <IconButton title="New workflow" icon="plus" size={32} className="sidebar-header-action" buttonRef={newButton} menuOpen={workflowHomes.length > 1 ? newMenu : undefined}
        onClick={() => (workflowHomes.length > 1 ? setNewMenu((o) => !o) : workflowHomes[0] && newWorkflow(workflowHomes[0].computer.id))} disabled={!workflowHomes.length} />,
      menu: newWorkflowItems,
      search: { label: 'Search', empty: 'No results', items: () => workflowItems(shownStores, openWorkflows) },
    },
  }
  const list = page === 'bots' || page === 'threads' || page === 'workflows' ? lists[page] : null
  const listed = list !== null
  // Search away from a list (its command) finds the bots.
  const search = (list ?? lists.bots).search
  // How the list shown is laid out, each list its own choice: the Threads list one grouping at a time.
  const layOut = (option: 'folder' | 'bot' | 'computer' | 'status', on: boolean) => {
    analytics.track({ event: 'list_layout_changed', list: page === 'threads' ? 'threads' : 'bots', option, on })
    if (page === 'threads') setThreadsGroup(on && option !== 'status' ? option : 'none')
    else if (option === 'status') setBotsByStatus(on)
    else setBotsByComputer(on)
  }
  const sortThreads = (option: 'status' | 'activity', on: boolean) => {
    analytics.track({ event: 'list_layout_changed', list: 'threads', option, on })
    if (option === 'status') setThreadsByStatus(on)
    else setThreadsByActivity(on)
  }
  // The computers shown that can move a bot with a new message to the top (hosts before 2.45.1 can't): theirs to
  // keep, so the choice reaches every device, unlike the two above.
  const ordering = page === 'bots'
    ? [...shown].flatMap((id) => app.store(id) ?? []).filter((s) => s.autoOrder != null && s.shownConnection.kind === 'online')
    : []
  const byActivity = ordering.length > 0 && ordering.every((s) => s.autoOrder)
  const listOptions: MenuItem[] = [
    ...(page === 'threads'
      ? [
          ...(['folder', 'bot', 'computer'] as const).map((by) => ({ title: ({ folder: 'Group by folder', bot: 'Group by bot', computer: 'Group by computer' } as const)[by], selected: threadsGroup === by, keepsOpen: true, action: () => layOut(by, threadsGroup !== by) })),
          // Beside the grouping: how the rows of each group are ordered.
          { title: 'Sort by status', divider: true, selected: threadsByStatus, keepsOpen: true, action: () => sortThreads('status', !threadsByStatus) },
          { title: 'Sort by last active', selected: threadsByActivity, keepsOpen: true, action: () => sortThreads('activity', !threadsByActivity) },
        ]
      : page !== 'bots' ? [] : [
          { title: 'Group by computer', selected: botsByComputer, keepsOpen: true, action: () => layOut('computer', !botsByComputer) },
          { title: 'Sort by status', selected: botsByStatus, keepsOpen: true, action: () => layOut('status', !botsByStatus) },
        ]),
    ...(ordering.length ? [{
      title: 'Sort by last active', selected: byActivity, keepsOpen: true,
      action: () => {
        analytics.track({ event: 'list_layout_changed', list: 'bots', option: 'activity', on: !byActivity })
        for (const store of ordering) void store.setAutoOrder(!byActivity)
      },
    }] : []),
  ]
  // Grouping or sorting the list another way slides its rows to their new places.
  useListFlip(sidebar, page, page === 'threads' ? `${threadsGroup}/${threadsByStatus}/${threadsByActivity}` : `${botsByComputer}/${botsByStatus}/${byActivity}`)
  const settingsPages: Partial<Record<AppPage, SettingsPage>> = { computers: 'computers', network: 'network', usage: 'usage', settings: settingsPage }
  return (
    <OpenExtensions.Provider value={openExtensions}>
    <div className={`split ${context ? 'covered' : ''}`}>
      <WindowBar
        back={step(visits, -1) ? () => travel(-1) : null}
        forward={step(visits, 1) ? () => travel(1) : null}
        toggleList={listed ? () => setListHidden(!listHidden) : null}
        setTitle={setBarTitle}
        setEnd={setBarEnd}
      />
      <NavRail page={page} go={go} working={threads.working} approvals={app.approvals.length} workflows={!!workflowStore} />
      <div className="page-frame">
      {list ? (
        <>
          <aside ref={sidebar} className={`sidebar ${resizing ? 'resizing' : ''}`} style={{ width }} inert={listHidden}>
            {/* Keeps its width while the sidebar opens or closes: the list slides under the edge instead of squeezing. */}
            <div className="sidebar-content" style={{ width: sidebarWidth }}>
              <div className="sidebar-header drag">
                <Icon name={pageIcon(page)} size={18} weight="semibold" className="sidebar-title crowded" />
                <Text size="title3" weight="semibold" tone="text" truncate className="sidebar-title crowded">{tr(list.title)}</Text>
                <Spacer />
                <HStack gap={6} className="no-drag">
                  <ComputerFilterHeader hidden={hidden} setHidden={setHidden} manage={() => setPage('computers')} options={listOptions} />
                  <IconButton title="Search" icon="magnifyingglass" size={32} className="sidebar-header-action" onClick={() => setSearching(true)} />
                  {list.add}
                </HStack>
                <AnchoredMenu open={newMenu} onClose={() => setNewMenu(false)} anchor={newButton} items={list.menu} />
              </div>
              {page === 'bots' ? (
                <div className="roster" ref={listRef} tabIndex={0} onKeyDown={onListKey}>
                  <BotRoster roster={roster}
                    onContext={(bot, store, x, y) => setContext({ items: botActions(bot, store), x, y })}
                    onSectionContext={(section, store, x, y) => {
                      const items = sectionHeaderItems(section, store, setNaming, (to) => roster.renameSection(store, section, to))
                      if (items.length) setContext({ items, x, y })
                    }}
                    openExtensions={openExtensions}
                    openWorkflows={openWorkflows} />
                  {visibleRoster.length === 0 && !roster.grouped && mismatchStores.length === 0 ? (
                    <div className="roster-empty">
                      <Text as="div" size="body" weight="medium">{onlineStores.length ? tr(`No bots yet`) : tr(`Waiting for a computer`)}</Text>
                      {onlineStores.length ? <Text as="div" size="callout" tone="secondary">{tr(`Use + to create a bot.`)}</Text> : null}
                    </div>
                  ) : null}
                </div>
              ) : page === 'workflows' ? (
                <div className="roster">
                  <WorkflowList stores={shownStores} place={shownWorkflow} open={openWorkflows} />
                </div>
              ) : (
                <div className="roster">
                  <ThreadList roster={threadRoster} picked={picked} pick={setPicked} start={startThread} openChat={(ref) => { app.select(ref); setPage('bots') }} />
                </div>
              )}
            </div>
          </aside>
          <ResizeHandle label={tr('Resize sidebar')} width={width} min={0} max={sidebarMax} edge="trailing" onResize={resizeSidebar} onDragging={setResizing} />
        </>
      ) : null}
      {/* The conversation stays as it is behind another page: its place, its open panel, a call under way. */}
      <main className="detail" hidden={page !== 'bots'}>
        {/* The call bar's settings open the Settings page at Voice chat. */}
        <WindowBarSlots.Provider value={page === 'bots' ? { title: barTitle, end: barEnd } : NO_BAR_SLOTS}>
          <OpenVoiceSettings.Provider value={openVoiceSettings}>{detail}</OpenVoiceSettings.Provider>
        </WindowBarSlots.Provider>
        {find.focus !== null && selectedBot ? <FindBar focusKey={find.focus} onClose={find.close} /> : null}
      </main>
      {page === 'threads' ? (
        <main className="detail">
          <WindowBarSlots.Provider value={{ title: barTitle, end: barEnd }}>
            <ThreadDetail roster={threadRoster} picked={picked} pick={setPicked} starting={starting} start={setStarting} openChat={(ref) => { app.select(ref); setPage('bots') }} />
          </WindowBarSlots.Provider>
        </main>
      ) : page === 'workflows' && workflowStore ? (
        <main className="detail">
          <StoreContext.Provider value={workflowStore}>
            <WindowBarSlots.Provider value={{ title: null, end: barEnd }}>
              <WorkflowsView
                key={workflowStore.computer.id}
                focus={workflowsOf === workflowStore.computer.id ? workflowFocus : null}
                onFocused={() => setWorkflowFocus(null)}
                shown={shownWorkflow?.computerId === workflowStore.computer.id ? shownWorkflow.workflowId : null}
                onShown={(workflowId) => setShownWorkflow({ computerId: workflowStore.computer.id, workflowId })}
                roster={threadRoster}
                openChat={(ref) => { app.select(ref); setPage('bots') }}
              />
            </WindowBarSlots.Provider>
          </StoreContext.Provider>
        </main>
      ) : page === 'workflows' ? (
        <main className="detail">
          <div className="empty-detail">
            <Text as="div" tone="secondary">{tr(`Waiting for a computer`)}</Text>
          </div>
        </main>
      ) : listed ? null : (
        <main className="app-page">
          <div className="app-page-drag drag" />
          {page !== 'extensions' ? (
            <SettingsView page={settingsPages[page] ?? settingsPage} setPage={setSettingsPage} />
          ) : extensionsStore ? (
            <StoreContext.Provider value={extensionsStore}>
              <MarketplaceView key={extensionsStore.computer.id} computer={extensionsChoice} focus={extensionsFocus} />
            </StoreContext.Provider>
          ) : (
            <div className="empty-detail">
              <Text as="div" tone="secondary">{tr(`Waiting for a computer`)}</Text>
            </div>
          )}
        </main>
      )}
      </div>

      {context ? (
        <div className="context-layer" onMouseDown={(e) => e.target === e.currentTarget && setContext(null)}>
          <DesktopActionMenu
            items={context.items}
            onDismiss={() => setContext(null)}
            x={context.x}
            y={context.y}
          />
        </div>
      ) : null}

      <SearchPalette
        open={searching}
        items={searching ? search.items() : []}
        label={search.label}
        empty={search.empty}
        onClose={() => setSearching(false)}
      />
      <Sheet open={editing !== null} onClose={() => setEditing(null)} width={520} height={Math.min(680, sheetHeight)}>
        {editing ? (
          <StoreContext.Provider value={editing.store}>
            <BotEditorView key={editing.id} draft={editing.draft} />
          </StoreContext.Provider>
        ) : null}
      </Sheet>
      <Sheet open={creating !== null} onClose={() => setCreating(null)} maxHeight={sheetHeight}>
        {creating && createStore ? (
          <StoreContext.Provider value={createStore}>
            {/* A group's bots belong to its computer: another computer starts it over. */}
            {creating.kind === 'bot' ? <NewBotView computer={newChoice} /> : <NewGroupView key={creating.computerId} computer={newChoice} />}
          </StoreContext.Provider>
        ) : null}
      </Sheet>
      <ImportBotsSheet store={localOnline ? app.local : null} open={importing} onClose={() => setImporting(false)} maxHeight={sheetHeight} />
      <Sheet open={screenStore !== null} onClose={() => app.openScreen(null)} width={windowSize.width - 48} height={sheetHeight}>
        {screenStore ? <ScreenViewer key={screenStore.computer.id} store={screenStore} watching={app.screenWatching} /> : null}
      </Sheet>
      <Dialog
        open={confirmDelete !== null}
        title={tr('Delete {name}?').replaceAll('{name}', () => confirmDelete?.bot.name ?? 'bot')}
        message={tr('Files it changed on your computer stay as they are.')}
        actions={confirmDelete ? [{ title: 'Delete bot and its conversation', destructive: true, action: () => confirmDelete.store.delete(confirmDelete.bot) }] : []}
        onClose={() => setConfirmDelete(null)}
      />
      <Dialog
        open={newSessionBot !== null}
        title={tr('Start a new session?')}
        message={tr('The conversation stays here, but the agent starts with a fresh context.')}
        actions={newSessionBot ? [{ title: 'New session', action: () => newSessionBot.store.newSession(newSessionBot.bot.id) }] : []}
        onClose={() => setNewSessionBot(null)}
      />
      <NameDialog naming={naming} placeholder="Section name" limit={limitSectionName} clean={sectionName} onClose={() => setNaming(null)} />
      <Dialog open={errorMessage !== null} title={tr('Something went wrong')} message={errorMessage} actions={[]} cancel="OK" onClose={() => app.clearErrors()} />
      <SignInCodeDialog />
    </div>
    </OpenExtensions.Provider>
  )
}
