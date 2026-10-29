import { useCallback, useEffect, useRef, useState, type ReactNode } from 'react'
import type { WindowCommand } from '@shared/ipc'
import { isSSHConnecting, showsChat as shouldShowChat } from '@shared/ssh-startup'
import { draftOf, type Bot, type BotDraft } from '@shared/models'
import { CharacterAvatar } from '../components/Avatar'
import { Button, IconButton, ResizeHandle } from '../components/Controls'
import { AnchoredMenu, Dialog, ModalHeader, Sheet, type MenuItem } from '../components/Overlay'
import { HStack, Spacer } from '../components/Stack'
import { Text } from '../components/Text'
import { useModels } from '../lib/observable'
import { prefs, stepTextSize, usePref, DEFAULT_TEXT_SIZE } from '../lib/prefs'
import { useApp, StoreContext } from '../store/context'
import { account } from '../store/account'
import { analytics } from '../store/analytics'
import { refKey, sameRef, parseRef } from '../store/app-model'
import type { BotStore } from '../store/bot-store'
import { useComputerRoster } from '../store/computer-roster'
import { BotRoster } from './BotRoster'
import { ExtensionsMovedTip } from './ExtensionsMovedTip'
import { SidebarFooter } from './SidebarFooter'
import { UpdateBadge } from './UpdateButton'
import { ComputerFilterHeader, computerSelection } from './ComputerFilterHeader'
import { ThreadView } from './thread/ThreadView'
import { queueVoiceKey } from './thread/voice-shortcut-keys'
import { NewBotView, NewGroupView } from './bots/NewBotView'
import { BotEditorView } from './bots/BotEditorView'
import { MarketplaceView } from './marketplace/MarketplaceView'
import { SettingsView, type SettingsPage } from './settings/SettingsView'
import { ApprovalSheet } from './settings/ApprovalSheet'
import { SearchPalette } from './SearchPalette'
import { AccountWelcomeView } from './AccountWelcomeView'
import { ComputerAccessView } from './computer-access/ComputerAccessView'
import { HostStateView } from './HostStateView'
import { StartupRecovery } from './StartupRecovery'
export { EmptyState } from './HostStateView'
import { RefreshIndicator } from './RefreshIndicator'
import { StarPrompt } from './StarPrompt'
import { OpenVoiceSettings } from './call/CallView'
import { AccountPanelLayer, DesktopActionMenu, ProfileAvatar } from './PanelMenus'
import { useTraySummary } from './tray-summary'
import { useIslandSummary } from './island-summary'
import { useSSH, useSSHAttachments } from './settings/ssh-model'
import './chat-window.css'
import './sidebar-header.css'
import { useTr } from '../lib/use-language'


/** Native desktop chat: roster on the left, the conversation on the right (Grok Bot's desktop layout). */
export function ChatWindow() {
  const tr = useTr()
  const app = useApp()
  const [onboarded, setOnboarded] = usePref(prefs.onboardingDone)
  const [accessSetup, setAccessSetup] = usePref(prefs.computerAccessSetup)
  const [showAccess, setShowAccess] = useState(false)
  const [confirmReset, setConfirmReset] = useState(false)
  const [upToDate, setUpToDate] = useState(false)
  useSSHAttachments(app)
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
          case 'openBot': {
            const ref = parseRef(c.ref)
            if (ref) app.select(ref)
            return
          }
          case 'stopBot': {
            const ref = parseRef(c.ref)
            if (ref) app.store(ref.computerId)?.stop(ref.botId)
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
      {onboarded ? (
        accessSetup === null ? (
          <ComputerAccessView onContinue={(ready) => finishAccess(ready, true)} />
        ) : showsChat ? (
          <ChatSplitView />
        ) : (
          <HostStateView />
        )
      ) : (
        <AccountWelcomeView
          onContinue={() => {
            setOnboarded(true)
            analytics.track({ event: 'onboarding_completed', signed_in: account.isSignedIn })
          }}
        />
      )}
      {onboarded && accessSetup !== null && showsChat ? null : <div className="window-drag drag" />}
      {onboarded && accessSetup !== null ? <StarPrompt /> : null}
      <Sheet open={showAccess} onClose={() => setShowAccess(false)} width={660} height={Math.min(820, window.innerHeight - 80)}>
        <ModalHeader icon="hand.raised" title={tr(`Computer access`)} />
        <ComputerAccessView onContinue={(ready) => { finishAccess(ready, false); setShowAccess(false) }} />
      </Sheet>
      <Dialog
        open={confirmReset}
        title={tr('Reset all data?')}
        message={tr("Signs out every account and deletes this computer's bots, conversations, keys and settings. Codync then starts over from the welcome screen.")}
        actions={[{ title: 'Reset everything', destructive: true, action: () => void window.codync.app.resetAllData() }]}
        onClose={() => setConfirmReset(false)}
      />
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
  const [compact, setCompact] = usePref(prefs.sidebarCompact)
  const [sidebarWidth, setSidebarWidth] = usePref(prefs.sidebarWidth)
  const [hidden, setHidden] = usePref(prefs.hiddenComputers)
  const [searching, setSearching] = useState(false)
  const [creating, setCreating] = useState<CreateTarget | null>(null)
  const [newMenu, setNewMenu] = useState(false)
  const [railNewMenu, setRailNewMenu] = useState(false)
  const [contextBot, setContextBot] = useState<{ target: BotTarget; x: number; y: number } | null>(null)
  const [editing, setEditing] = useState<EditTarget | null>(null)
  const [confirmDelete, setConfirmDelete] = useState<BotTarget | null>(null)
  const [newSessionBot, setNewSessionBot] = useState<BotTarget | null>(null)
  const [marketplace, setMarketplace] = useState<string | null>(null)
  const [settings, setSettings] = useState<SettingsPage | null>(null)
  const openVoiceSettings = useCallback(() => setSettings('voice'), [])
  const [showAccount, setShowAccount] = useState(false)
  const [windowSize, setWindowSize] = useState({ width: window.innerWidth, height: window.innerHeight })
  const newButton = useRef<HTMLButtonElement>(null)
  const railNewButton = useRef<HTMLButtonElement>(null)
  const listRef = useRef<HTMLDivElement>(null)
  useModels([...app.stores.values()])

  useEffect(() => {
    const onResize = () => {
      setWindowSize({ width: window.innerWidth, height: window.innerHeight })
      setSidebarWidth(Math.min(prefs.sidebarWidth.get(), Math.max(260, window.innerWidth - 420)))
    }
    window.addEventListener('resize', onResize)
    return () => window.removeEventListener('resize', onResize)
  }, [setSidebarWidth])

  const sheetHeight = Math.max(420, windowSize.height - 76)
  const stores = app.computers.map((c) => app.store(c.id)).filter((s): s is BotStore => !!s)
  const shown = new Set(computerSelection(app.computers.map((c) => c.id), hidden).shown)
  const onlineStores = stores.filter((s) => shown.has(s.computer.id) && s.connection.kind === 'online' && !s.mismatch)
  const selectedStore = app.selectedStore
  const createStore = creating ? app.store(creating.computerId) : null
  const roster = useComputerRoster(app, shown)
  const visibleRoster = roster.items

  const create = (kind: CreateTarget['kind']) => {
    // The selected bot's computer if it's online, else this computer, else any online one.
    const target = [selectedStore, app.local].find((s) => s && onlineStores.includes(s)) ?? onlineStores[0]
    if (target) setCreating({ kind, computerId: target.computer.id })
  }
  const computerChoice = creating && onlineStores.length > 1
    ? { options: onlineStores.map((s) => ({ id: s.computer.id, label: s.hostName })), selection: creating.computerId, onChange: (computerId: string) => setCreating({ ...creating, computerId }) }
    : null

  useEffect(
    () =>
      window.codync.app.onCommand((c) => {
        if (c.kind === 'newBot' && onlineStores.length) create('bot')
        if (c.kind === 'search') setSearching(true)
        if (c.kind === 'networkSettings') setSettings('network')
        if (c.kind === 'toggleSidebar') setCompact(!prefs.sidebarCompact.get())
        if (c.kind === 'voiceKey') {
          // No chat open: talk to the top bot in the sidebar.
          if (c.key === 'double' && !app.selection) app.select((roster.navigable[0] ?? roster.items[0])?.ref ?? null)
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
    } else if (target === 'plugins') {
      if (!app.local) return
      debugOpened.current = true
      setMarketplace(app.local.computer.id)
    } else if (target === 'computers') {
      debugOpened.current = true
      setSettings('computers')
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
  ]

  const openMarketplace = () => setMarketplace((selectedStore ?? app.local ?? onlineStores[0])?.computer.id ?? null)

  // The lead stays on top and can't go: no pin, hide or delete.
  const botActions = (bot: Bot, store: BotStore): MenuItem[] => [
    ...(bot.lead ? [] : [{ title: bot.pinned ? 'Unpin' : 'Pin', icon: 'pin', action: () => store.setPinned(bot, !bot.pinned) }]),
    { title: 'Mark as Read', icon: 'bell.badge', action: () => store.markAllRead(bot.id) },
    { title: 'Edit Profile…', icon: 'square.and.pencil', divider: true, action: () => setEditing({ id: crypto.randomUUID(), draft: draftOf(bot), store }) },
    { title: 'Copy conversation ID', icon: 'square.on.square', divider: true, action: () => window.codync.app.copy(bot.id) },
    { title: 'New session', icon: 'arrow.counterclockwise', action: () => setNewSessionBot({ bot, store }) },
    ...(bot.lead ? [] : [
      { title: 'Hide from sidebar', icon: 'eye.slash', divider: true, action: () => store.setHidden(bot, true) },
      { title: 'Delete', icon: 'trash', destructive: true, action: () => setConfirmDelete({ bot, store }) },
    ]),
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

  // The sidebar's edge drags its width; narrower than 180 collapses it to the rail.
  const [resizing, setResizing] = useState(false)
  const sidebarMax = Math.min(420, window.innerWidth - 420)
  const resizeSidebar = (proposed: number) => {
    const nextCompact = proposed < 180
    if (nextCompact !== compact) setCompact(nextCompact)
    if (!nextCompact) setSidebarWidth(Math.min(Math.max(260, proposed), sidebarMax))
  }

  const errorMessage = app.errorMessage
  const mismatchStores = stores.filter((s) => shown.has(s.computer.id) && s.mismatch)
  const selected = app.selection
  const selectedBot = selected && selectedStore?.bots.get(selected.botId)

  let detail: ReactNode
  if (selected && selectedStore && selectedBot) {
    detail = (
      <StoreContext.Provider value={selectedStore}>
        <ThreadView key={refKey(selected)} botId={selected.botId} />
      </StoreContext.Provider>
    )
  } else if (!onlineStores.length && ssh.state.profiles.length) {
    detail = <StartupRecovery manage={() => setSettings('computers')} />
  } else {
    detail = (
      <div className="empty-detail">
        <div className="detail-drag drag" />
        <HStack align="stretch" className="empty-detail-crew">
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

  const width = compact ? 76 : sidebarWidth
  return (
    <div className={`split ${showAccount || contextBot ? 'covered' : ''}`}>
      <aside className={`sidebar ${compact ? 'compact' : ''} ${resizing ? 'resizing' : ''}`} style={{ width }}>
        <div className="sidebar-header drag" inert={compact}>
          <Spacer />
          <HStack gap={6} className="no-drag">
            <ComputerFilterHeader hidden={hidden} setHidden={setHidden} manage={() => setSettings('computers')} compact />
            <IconButton title="Search" icon="magnifyingglass" size={32} className="sidebar-header-action" onClick={() => setSearching(true)} />
            <IconButton title="New" icon="plus" size={32} className="sidebar-header-action" buttonRef={newButton} menuOpen={newMenu} onClick={() => setNewMenu((o) => !o)} disabled={!onlineStores.length} />
          </HStack>
          <AnchoredMenu open={newMenu} onClose={() => setNewMenu(false)} anchor={newButton} items={newItems} />
        </div>
        {compact ? (
          <div className="rail-header drag">
            <HStack align="stretch" justify="center" className="no-drag">
              <ComputerFilterHeader hidden={hidden} setHidden={setHidden} manage={() => setSettings('computers')} compact />
            </HStack>
          </div>
        ) : null}
        <div className="roster" ref={listRef} tabIndex={0} onKeyDown={onListKey}>
          {roster.grouped && !compact ? <ExtensionsMovedTip /> : null}
          <BotRoster roster={roster} compact={compact} onContext={(bot, store, x, y) => setContextBot({ target: { bot, store }, x, y })} openExtensions={(id) => {
            // Found it: the tip about where Extensions went has done its job.
            prefs.extensionsMovedTipSeen.set(true)
            setMarketplace(id)
          }} />
          {visibleRoster.length === 0 && !roster.grouped && !compact && mismatchStores.length === 0 ? (
            <div className="roster-empty">
              <Text as="div" size="body" weight="medium">{onlineStores.length ? tr(`No bots yet`) : tr(`Waiting for a computer`)}</Text>
              {onlineStores.length ? <Text as="div" size="callout" tone="secondary">{tr(`Use + to create a bot.`)}</Text> : null}
            </div>
          ) : null}
        </div>
        {compact ? (
          <div className="rail-actions">
            <IconButton title="Search" icon="magnifyingglass" onClick={() => setSearching(true)} />
            <IconButton title="New" icon="plus" size={36} buttonRef={railNewButton} menuOpen={railNewMenu} disabled={!onlineStores.length} onClick={() => setRailNewMenu((o) => !o)} />
            <AnchoredMenu open={railNewMenu} onClose={() => setRailNewMenu(false)} anchor={railNewButton} items={newItems} />
            <IconButton title="Extensions" icon="square.grid.2x2" size={36} onClick={openMarketplace} />
            <UpdateBadge size={32} />
            <button className={`rail-profile ${showAccount ? 'open' : ''}`} aria-label="Open account menu for Account" title="Account" onClick={() => setShowAccount((s) => !s)}>
              <ProfileAvatar />
            </button>
          </div>
        ) : (
          // With several computers each heading opens its own Extensions instead.
          <SidebarFooter accountOpen={showAccount} onAccount={() => setShowAccount((s) => !s)} onExtensions={roster.grouped ? undefined : openMarketplace} />
        )}
      </aside>
      <ResizeHandle label="Resize sidebar" width={width} min={76} max={sidebarMax} edge="trailing" onResize={resizeSidebar} onDragging={setResizing} />
      <main className="detail">
        {/* The call bar's settings open this window's Settings at Voice chat. */}
        <OpenVoiceSettings.Provider value={openVoiceSettings}>{detail}</OpenVoiceSettings.Provider>
      </main>

      {showAccount ? (
        <AccountPanelLayer
          compact={compact}
          approvals={app.approvals.length}
          onDismiss={() => setShowAccount(false)}
          onUsage={() => {
            setShowAccount(false)
            setSettings('usage')
          }}
          onSettings={() => {
            setShowAccount(false)
            setSettings('general')
          }}
        />
      ) : null}
      {contextBot ? (
        <div className="context-layer" onMouseDown={(e) => e.target === e.currentTarget && setContextBot(null)}>
          <DesktopActionMenu
            items={botActions(contextBot.target.bot, contextBot.target.store)}
            onDismiss={() => setContextBot(null)}
            style={{ left: Math.min(Math.max(8, contextBot.x), windowSize.width - 268), top: Math.min(Math.max(8, contextBot.y), Math.max(8, windowSize.height - 344)) }}
          />
        </div>
      ) : null}

      <SearchPalette open={searching} items={visibleRoster} onPick={(ref) => app.select(ref)} onClose={() => setSearching(false)} />
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
            {creating.kind === 'bot' ? <NewBotView computer={computerChoice} /> : <NewGroupView key={creating.computerId} computer={computerChoice} />}
          </StoreContext.Provider>
        ) : null}
      </Sheet>
      <Sheet open={marketplace !== null} onClose={() => setMarketplace(null)} width={Math.min(920, windowSize.width - 80)} height={sheetHeight} dismissOnDim>
        {marketplace && app.store(marketplace) ? (
          <StoreContext.Provider value={app.store(marketplace)!}>
            <MarketplaceView key={marketplace} />
          </StoreContext.Provider>
        ) : null}
      </Sheet>
      <Sheet open={settings !== null} onClose={() => setSettings(null)} width={Math.min(800, windowSize.width - 80)} height={Math.min(620, sheetHeight)}>
        {settings ? <SettingsView page={settings} setPage={setSettings} /> : null}
      </Sheet>
      <Dialog
        open={newSessionBot !== null}
        title={tr('Start a new session?')}
        message={tr('The conversation stays here, but the agent starts with a fresh context.')}
        actions={newSessionBot ? [{ title: 'New session', action: () => newSessionBot.store.newSession(newSessionBot.bot.id) }] : []}
        onClose={() => setNewSessionBot(null)}
      />
      <Dialog
        open={confirmDelete !== null}
        title={`Delete ${confirmDelete?.bot.name ?? 'bot'}?`}
        message={tr('Files it changed on your computer stay as they are.')}
        actions={confirmDelete ? [{ title: 'Delete bot and its conversation', destructive: true, action: () => confirmDelete.store.delete(confirmDelete.bot) }] : []}
        onClose={() => setConfirmDelete(null)}
      />
      <Dialog open={errorMessage !== null} title={tr('Something went wrong')} message={errorMessage} actions={[]} cancel="OK" onClose={() => app.clearErrors()} />
    </div>
  )
}
