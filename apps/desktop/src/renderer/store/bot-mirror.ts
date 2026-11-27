import type { HostAlert } from '@shared/alerts'
import { checkVersions, isBelow, type VersionMismatch } from '@shared/compat'
import { mixesAgents } from '@shared/fallbacks'
import {
  type AccessRequest,
  type Bot,
  type CloudStatus,
  type Computer,
  type Entry,
  type Hello,
  type PairingRequest,
  type ScreenState,
  type Usage,
  normalizeBot,
  withoutRemoved,
} from '@shared/models'
import type { NetworkView, RecoveryCode } from '@shared/network'
import { HostError, type HostClient, type HostRoute, type HostTransport } from '../client/host-client'
import type { CloudPlacement } from '../lib/cloud-computer'
import type { RunDetail, WorkflowDetail, WorkflowStep, WorkflowSummary } from '../views/workflows/workflow'
import { rosterOrder } from '../lib/computer-roster'
import { HOST_UPDATE_WAIT, hostVersionLine, offerHostUpdate, type HostUpdate, type HostUpdateOffer, type HostVersionLine } from '../lib/host-update'
import { Observable } from '../lib/observable'
import { hostHasSections } from '../lib/roster-sections'
import { ComposerDrafts } from '@shared/composer-drafts'
import { canLoadHistory } from '@shared/mirror-window'
import { fromOldBot, fromOldEntry } from '@shared/old-host'
import { loadIconCache, saveIconCache } from '../lib/icon-cache'
import { trNow } from '../lib/l10n.ts'
import { ThreadTranscripts } from './thread-transcripts'
import { OlderThreads } from './older-threads'
import type { AgentModels } from '@shared/agent-models'

/** The first host where the user picks the lead and may delete it. */
const LEAD_CHOICE_HOST = '2.33.0'
/** The first host the user names (`setComputerName`). */
const COMPUTER_NAME_HOST = '2.50.0'

export type Connection =
  | { kind: 'unpaired' }
  | { kind: 'connecting' }
  | { kind: 'online' }
  /** `waking`: the relay is starting it (a cloud computer asleep). */
  | { kind: 'computerOffline'; lastSeen: number | null; waking?: boolean }
  | { kind: 'offline'; message: string }
  | { kind: 'unauthorized'; message: string }

/**
 * The state half of `BotStore`: one computer's mirror (bots, transcripts), the selectors views
 * read, entry merging and the on-device cache.
 */
export class BotMirror extends Observable {
  icons: import('@shared/custom-icons').CustomIcons = {}
  private iconsReceived = false
  protected iconsRevision = -1
  iconsEpoch = 0

  applyIcons(icons: import('@shared/custom-icons').CustomIcons, revision = 0) {
    if (revision < this.iconsRevision) return
    this.iconsRevision = revision
    this.iconsReceived = true
    void saveIconCache(this.cacheKey, icons).catch((error: unknown) => console.warn('Could not cache custom icons', error))
    this.restoreIcons(icons)
  }

  private restoreIcons(icons: import('@shared/custom-icons').CustomIcons) {
    this.icons = icons
    for (const [id, bot] of this.bots) this.bots.set(id, this.withIcon(bot))
    this.onRosterChanged?.()
    this.changed()
  }

  /** Icons sync apart from bots, so no bot the host sends carries its custom icon. */
  private withIcon(bot: Bot): Bot {
    return { ...bot, customIcon: this.icons[`bot:${bot.id}`] }
  }

  /** Stores a bot the host sent, with its custom icon. */
  protected putHostBot(bot: Bot): Bot {
    this.olderThreads.keep(bot.id, this.bots.get(bot.id)?.work, bot.work)
    const stored = this.withIcon(bot)
    this.bots.set(stored.id, stored)
    return stored
  }
  computer: Computer
  connection: Connection = { kind: 'connecting' }
  hostRoute: HostRoute | null = null
  /** Set for a Codync Cloud computer: who runs it, where, and whether it's awake (the account's computer list). */
  cloudHosting: CloudPlacement | null = null
  client: HostClient | null = null
  hello: Hello | null = null
  hostVersion: { version: string; minApp?: string | null } | null = null
  /** An update asked of this computer from here while it's behind; cleared once its version changes. */
  hostUpdate: HostUpdate | null = null
  bots = new Map<string, Bot>()
  entries = new Map<string, Entry[]>()
  private threadTranscripts = new ThreadTranscripts()
  /** Ended threads from before the last day, listed on the Threads page (`hello.threadKeepDays` hosts). */
  readonly olderThreads = new OlderThreads(() => this.changed())
  usage: Usage = { providers: [] }
  /** The agents' model lists this computer gave, by agent and login profile: a model picker opens on one at once. */
  private readonly modelLists = new Map<string, AgentModels>()
  screen: ScreenState | null = null
  accessRequests: AccessRequest[] = []
  pairingRequests: PairingRequest[] = []
  cloud: CloudStatus | null = null
  /** This computer's network; null until known, and on hosts without `networkView`. */
  network: NetworkView | null = null
  /** A new network's recovery code until the user says they saved it: the host keeps it nowhere. */
  recoveryCode: RecoveryCode | null = null
  historyComplete = new Set<string>()
  readonly composerDrafts: ComposerDrafts
  lastError: string | null = null
  /** Set when `lastError` must not open the generic failure dialog. */
  quietError = false
  workflows: WorkflowSummary[] = []
  workflowMethodMissing = false
  workflowSession: { id: string; dirty: boolean; runId: string | null; followLive: boolean } | null = null
  workflowDetail: WorkflowDetail | null = null
  workflowDetailToken = 0
  workflowRunToken = 0
  workflowRun: RunDetail | null = null
  /** The latest state of a step run on its own ("Run this step"): it belongs to no run, so `workflowRun` never carries it. */
  workflowSoloStep: WorkflowStep | null = null
  /** The host predates `workflowStep`: a solo step's event can't be read back, while every other workflow method works. */
  workflowStepMissing = false
  workflowDeleted: string | null = null
  /** A kit view selected a bot on this store (new bot or group, editor); the account picks it up. */
  selection: string | null = null
  /** Permission cards whose answer is on its way, with the chosen option. */
  answering = new Map<string, string>()

  onRosterChanged: (() => void) | null = null
  /** The host's alerts for bots no chat on screen shows (a system notification, if the user turned those on). */
  onAlert: ((alert: HostAlert) => void) | null = null
  onComputerChanged: ((c: Computer) => void) | null = null

  protected waiting = 0
  protected rev = 0
  /** While everything is fetched again (`refetchAll`): the entries the computer has sent since it began. */
  protected refetched: Set<string> | null = null
  protected hostId: string | null = null
  protected retired = false
  protected unreadable = false
  protected saveTimer: ReturnType<typeof setTimeout> | null = null
  /** What the mirror was synced under (`syncStamp`): from the cache, then from each hello. Null: nothing held yet. */
  protected cacheStamp: string | null = null

  constructor(
    computer: Computer,
    readonly isLoopback: boolean,
    protected makeTransport: () => HostTransport,
    protected appVersion = window.codync.appVersion,
    readonly contextId = 'local',
  ) {
    super()
    this.computer = computer
    this.composerDrafts = new ComposerDrafts(localStorage, `codync-drafts-v1-${JSON.stringify([contextId, computer.id])}`,
      () => this.setError(trNow("Couldn't save the draft on this device. Your text is still here; keep this window open until you send or copy it.")))
    this.loadCache()
  }

  // MARK: derived

  get isOffline() {
    const k = this.connection.kind
    return k === 'computerOffline' || k === 'offline' || k === 'unauthorized'
  }

  /** Loopback has no relay mailbox. */
  get canQueue() {
    return false
  }

  /** What headers show: a reconnect the grace period keeps quiet reads "Connecting…" while an action waits. */
  get shownConnection(): Connection {
    return this.waiting > 0 && this.connection.kind === 'online' ? { kind: 'connecting' } : this.connection
  }

  get hostName() {
    return this.hello?.name ?? this.computer.name
  }

  /** The automatic bot order switch as the computer has it: switched here, or its `autoOrder` event (another device). */
  showAutoOrder(on: boolean) {
    if (!this.hello || this.hello.autoOrder === on) return
    this.hello = { ...this.hello, autoOrder: on }
    this.changed()
  }

  /** Hop-limit review as the computer has it: switched here, or its `hopReview` event (another device). */
  showHopReview(on: boolean) {
    if (!this.hello || this.hello.hopReview === on) return
    this.hello = { ...this.hello, hopReview: on }
    this.changed()
  }

  /** When old threads are deleted as the computer has it: chosen here, or its `threadKeepDays` event (another device). */
  showThreadKeepDays(days: number) {
    if (!this.hello || this.hello.threadKeepDays === days) return
    this.hello = { ...this.hello, threadKeepDays: days }
    // The computer deletes what the new choice no longer keeps: list them again.
    this.olderThreads.reset()
    this.changed()
  }

  /** `backend`'s model list for `loginProfile` (null: the default login) as this computer gave it last. */
  modelsFor(backend: string, loginProfile: string | null): AgentModels | null {
    return this.modelLists.get(`${backend}/${loginProfile ?? ''}`) ?? null
  }

  keepModels(backend: string, loginProfile: string | null, list: AgentModels) {
    this.modelLists.set(`${backend}/${loginProfile ?? ''}`, list)
    this.changed()
  }

  /** A new name for this computer: its host's `name` event, or a cloud computer renamed from here (shown before its host,
   * asleep perhaps, says so). */
  setHostName(name: string) {
    if (!this.hello || this.hello.name === name) return
    this.hello = { ...this.hello, name }
    this.changed()
  }

  setCloudHosting(hosting: CloudPlacement | null) {
    if (JSON.stringify(hosting) === JSON.stringify(this.cloudHosting)) return
    this.cloudHosting = hosting
    this.changed()
  }

  /** A Codync Cloud computer that's asleep, or on its way up: a connection or a message wakes it, so it isn't offline. */
  get isAsleep() {
    return this.connection.kind === 'computerOffline' && (this.isWaking || this.cloudHosting?.state === 'stopped' || this.cloudHosting?.state === 'stopping')
  }

  /** The relay is waking this cloud computer (about 10 to 30 seconds): it's coming, not offline. Only the relay says so:
   * the account list's `starting` stays after a wake whose host never came back. */
  get isWaking() {
    return this.connection.kind === 'computerOffline' && this.connection.waking === true
  }

  /** A Codync Cloud computer older than this app's `minHost`: the cloud updates it on its own (this app's `Codync-App` header asks for it). */
  get isCloudUpdating() {
    return this.mismatch?.kind === 'updateHost' && this.cloudHosting !== null
  }

  get mismatch(): VersionMismatch | null {
    if (!this.hostVersion) return null
    const found = checkVersions(this.appVersion, this.hostVersion.version, this.hostVersion.minApp)
    if (found) return found
    if (this.unreadable && isBelow(this.appVersion, this.hostVersion.version)) return { kind: 'updateApp', minimum: this.hostVersion.version }
    return null
  }

  /** The host keeps roster sections (an older one leaves them out): the menus offer them only then. */
  get hasRosterSections() {
    return this.hostVersion !== null && hostHasSections(this.hostVersion.version)
  }

  /** The user picks the lead (`setLead`) and may delete it: an older host refuses both. */
  get hasLeadChoice() {
    return this.hostVersion !== null && !isBelow(this.hostVersion.version, LEAD_CHOICE_HOST)
  }

  /** The user names this computer here, on a host that takes it: a cloud computer is named in Codync Cloud. */
  get canRename() {
    return this.connection.kind === 'online' && this.hello?.device !== 'cloud'
      && this.hostVersion !== null && !isBelow(this.hostVersion.version, COMPUTER_NAME_HOST)
  }

  /** What the update notice offers while this computer is behind this app. */
  get hostUpdateOffer(): HostUpdateOffer {
    return offerHostUpdate(this.hostUpdate, this.hello?.canUpdate === true)
  }

  /** This computer's Codync row at `now` (Settings > Computers); null until its `hello` arrives. */
  hostVersionLine(now = Date.now()): HostVersionLine | null {
    if (!this.hello) return null
    return hostVersionLine(this.hello.version, this.hello.latestVersion, this.mismatch,
      offerHostUpdate(this.hostUpdate, this.hello.canUpdate === true, now), this.hostName, this.hello.os, this.cloudHosting !== null)
  }

  /** Out of reach while an update asked from here installs: the computer restarts on its own, so it
   *  reads as updating, not as a lost connection (kit's `BotStore.isUpdating`). */
  get isUpdating(): boolean {
    return this.connection.kind !== 'online' && this.hostUpdateOffer.kind === 'updating'
  }

  /** When an update asked from here stops reading as under way; null when none was asked. */
  get hostUpdateWaitEnds(): number | null {
    return this.hostUpdate?.kind === 'requested' ? this.hostUpdate.at + HOST_UPDATE_WAIT : null
  }

  /** Roster order: the lead, then pinned bots, then where the user dragged them, then most recent activity. */
  get roster(): Bot[] {
    return [...this.bots.values()].filter((b) => !b.hidden).sort(rosterOrder)
  }

  /** The visible bots run more than one agent, so the roster marks each with its agent's logo. Not before
   * `hello`: it names the logos, and without it every badge would be the same generic one. */
  get mixesAgents(): boolean {
    return this.hello !== null && mixesAgents([...this.bots.values()].filter((b) => !b.hidden))
  }

  get hiddenBots(): Bot[] {
    return [...this.bots.values()].filter((b) => b.hidden && !b.plain).sort((a, b) => a.name.localeCompare(b.name))
  }

  /** The computer's box for threads without a bot, once the first one started. */
  get plainBox(): Bot | undefined {
    return [...this.bots.values()].find((b) => b.plain)
  }

  backendName(id: string) {
    return this.hello?.backends.find((b) => b.id === id)?.name ?? builtInBackendName(id)
  }

  /** The ACP registry id an agent's logo is under (`AgentIcon`). */
  agentRegistry(id: string) {
    return this.hello?.backends.find((b) => b.id === id)?.registry ?? id
  }

  /** A bot's or group's chat, trace entries included. */
  allEntries(botId: string) {
    const list = this.entries.get(botId)
    return list ? withoutRemoved(list) : []
  }

  /** Not while everything is fetched again (`refetchAll`): a page would sit below entries about to go. */
  canLoadOlder(botId: string) {
    return !this.refetched && canLoadHistory(this.allEntries(botId), this.historyComplete.has(botId))
  }

  members(group: Bot) {
    return group.members.map((id) => this.bots.get(id)).filter((b): b is Bot => !!b)
  }

  /** A bot's name; `fallback` for one that isn't this computer's (a bot on another computer). */
  authorName(id: string | null | undefined, fallback?: string) {
    return (id && this.bots.get(id)?.name) || fallback || trNow('A deleted bot')
  }

  get connectionLabel() {
    if (this.isWaking) return 'Waking the computer…'
    if (this.isAsleep) return 'Sleeping'
    switch (this.shownConnection.kind) {
      case 'online':
        return this.isCloudUpdating ? 'Updating…' : this.mismatch ? 'Needs update' : 'Connected'
      case 'connecting':
        return 'Connecting…'
      case 'computerOffline':
      case 'offline':
        return 'Offline'
      case 'unauthorized':
        return 'No access'
      case 'unpaired':
        return 'Not paired'
    }
  }

  updateComputer(change: (c: Computer) => Computer) {
    const next = change({ ...this.computer })
    if (JSON.stringify(next) === JSON.stringify(this.computer)) return
    this.computer = next
    this.onComputerChanged?.(next)
    this.changed()
  }

  protected upsert(e: Entry) {
    this.refetched?.add(e.id)
    const list = [...(this.entries.get(e.botId) ?? [])]
    const i = list.findIndex((x) => x.id === e.id)
    if (i >= 0) {
      // A late API response mustn't undo a newer SSE update.
      if (e.rev < list[i]!.rev) return
      list[i] = e
    } else {
      // Replace the optimistic copy of a message we sent.
      const nonce = e.kind === 'user' ? e.data.clientNonce : undefined
      const filtered = nonce ? list.filter((x) => x.id !== `local-${nonce}`) : list
      const last = filtered[filtered.length - 1]
      if (last && last.seq > e.seq && e.seq > 0) {
        const at = filtered.findIndex((x) => x.seq > e.seq)
        filtered.splice(at < 0 ? filtered.length : at, 0, e)
      } else {
        filtered.push(e)
      }
      this.entries.set(e.botId, filtered)
      return
    }
    this.entries.set(e.botId, list)
  }

  /** Holds a thread's transcript from now until the returned function is called: its details are open. */
  watchThread(threadId: string): () => void {
    return this.threadTranscripts.watch(threadId)
  }

  /** A watched thread's transcript, oldest first. */
  threadEntriesOf(threadId: string): Entry[] {
    return this.threadTranscripts.of(threadId)
  }

  /** More of a thread's transcript (fetched, or a live `threadEntry`). */
  protected mergeThread(threadId: string, entries: Entry[]) {
    this.threadTranscripts.merge(threadId, entries)
  }

  setError(message: string | null) {
    this.quietError = false
    this.lastError = message
    this.changed()
  }

  /** The open workflow. Dirty flips do not re-render the rest of the window. */
  setWorkflowSession(session: { id: string; dirty: boolean; runId: string | null; followLive: boolean } | null) {
    const idChanged = this.workflowSession?.id !== session?.id
    this.workflowSession = session
    if (idChanged) this.changed()
  }

  upsertWorkflow(workflow: WorkflowSummary) {
    const summary = { id: workflow.id, title: workflow.title, revision: workflow.revision, enabled: workflow.enabled, updatedAt: workflow.updatedAt, running: workflow.running, lastRun: workflow.lastRun }
    this.workflows = [summary, ...this.workflows.filter((item) => item.id !== workflow.id)].sort((a, b) => b.updatedAt - a.updatedAt)
    this.changed()
  }

  removeWorkflow(id: string) {
    this.workflows = this.workflows.filter((item) => item.id !== id)
    this.changed()
  }

  clearWorkflowDeleted() {
    this.workflowDeleted = null
  }

  /** The detail the open canvas already applied. Does not count as a remote graph change. */
  rememberWorkflowDetail(workflow: WorkflowDetail) {
    this.workflowDetail = workflow
    this.changed()
  }

  /** A host without the workflow methods hides the entry and keeps the sentence off the generic dialog. */
  reportWorkflowFailure(error: unknown) {
    if (!(error instanceof HostError) || error.message !== 'unknown method') {
      console.error('workflow', error)
      return false
    }
    this.workflowMethodMissing = true
    this.quietError = true
    this.lastError = error.message
    this.workflows = []
    this.changed()
    return true
  }

  protected resetWorkflowMirror() {
    this.workflows = []
    this.workflowMethodMissing = false
    this.workflowSession = null
    this.workflowDetail = null
    this.workflowRun = null
    this.workflowSoloStep = null
    this.workflowStepMissing = false
    this.workflowDeleted = null
    if (this.quietError) {
      this.quietError = false
      this.lastError = null
    }
  }

  setNetwork(network: NetworkView | null) {
    this.network = network
    this.changed()
  }

  setRecoveryCode(code: RecoveryCode | null) {
    this.recoveryCode = code
    this.changed()
  }

  // MARK: cache (instant launch, offline reading)

  private get cacheKey() {
    return `codync-mirror-${this.contextId}-${this.computer.id}`
  }

  private loadCache() {
    void loadIconCache(this.cacheKey).then((icons) => {
      if (!this.retired && !this.iconsReceived) this.restoreIcons(icons)
    }).catch((error: unknown) => console.warn('Could not read cached icons', error))
    try {
      const raw = localStorage.getItem(this.cacheKey)
      if (!raw) return
      const cache = JSON.parse(raw) as { stamp?: string; hostId?: string; rev: number; bots: Bot[]; entries: Entry[] }
      this.cacheStamp = cache.stamp ?? ''
      this.hostId = cache.hostId ?? null
      this.rev = cache.rev
      // A copy cached by an older app lacks the fields added since. One from before 2.51.0 has the old names
      // (hosts before 2.51.0: `fromOld…` goes when MIN_HOST reaches 2.51.0).
      this.bots = new Map(cache.bots.map((b) => [b.id, normalizeBot(fromOldBot(b) as Bot)]))
      const grouped = new Map<string, Entry[]>()
      for (const e of cache.entries.map(fromOldEntry)) grouped.set(e.botId, [...(grouped.get(e.botId) ?? []), e])
      this.entries = grouped
    } catch {}
  }

  protected scheduleSave() {
    if (this.retired) return
    if (this.saveTimer) clearTimeout(this.saveTimer)
    this.saveTimer = setTimeout(() => this.saveCache(), 2000)
  }

  saveCache() {
    if (this.retired) return
    // Keep the newest 200 entries per bot; older ones page in from the host.
    const kept: Entry[] = []
    for (const list of this.entries.values()) {
      kept.push(...list.filter((e) => !e.id.startsWith('local-')).slice(-200))
      kept.push(...list.filter((e) => e.id.startsWith('local-') && e.data.status === 'failed'))
    }
    const bots = [...this.bots.values()].map(({ customIcon: _image, ...bot }) => bot)
    // Saved partway through a refetch, the cache says so: the next launch begins it again.
    const stamp = this.refetched ? '' : this.cacheStamp ?? ''
    const cache = { stamp, hostId: this.hostId, rev: this.rev, bots, entries: kept }
    try {
      localStorage.setItem(this.cacheKey, JSON.stringify(cache))
    } catch {}
  }
}

export function builtInBackendName(id: string) {
  switch (id) {
    case 'claude':
      return 'Claude Code'
    case 'codex':
      return 'Codex'
    case 'opencode':
      return 'OpenCode'
    case 'grok':
      return 'Grok Build'
    case 'gemini':
      return 'Gemini CLI'
    default:
      return trNow('Custom agent')
  }
}
