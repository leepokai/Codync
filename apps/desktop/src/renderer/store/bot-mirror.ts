import { checkVersions, isBelow, type VersionMismatch } from '@shared/compat'
import {
  type AccessRequest,
  type Bot,
  type CloudStatus,
  type Computer,
  type Entry,
  type Hello,
  type ScreenState,
  type Usage,
} from '@shared/models'
import type { NetworkView, RecoveryCode } from '@shared/network'
import type { HostClient, HostRoute, HostTransport } from '../client/host-client'
import { rosterOrder } from '../lib/computer-roster'
import { HOST_UPDATE_WAIT, hostVersionLine, offerHostUpdate, type HostUpdate, type HostUpdateOffer, type HostVersionLine } from '../lib/host-update'
import { Observable } from '../lib/observable'
import { hostHasSections } from '../lib/roster-sections'
import { ComposerDrafts } from '@shared/composer-drafts'
import { canLoadHistory } from '@shared/mirror-window'
import { loadIconCache, saveIconCache } from '../lib/icon-cache'

/** The first host where the user picks the lead and may delete it. */
const LEAD_CHOICE_HOST = '2.33.0'

export type Connection =
  | { kind: 'unpaired' }
  | { kind: 'connecting' }
  | { kind: 'online' }
  | { kind: 'computerOffline'; lastSeen: number | null }
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
    const stored = this.withIcon(bot)
    this.bots.set(stored.id, stored)
    return stored
  }
  computer: Computer
  connection: Connection = { kind: 'connecting' }
  hostRoute: HostRoute | null = null
  client: HostClient | null = null
  hello: Hello | null = null
  hostVersion: { version: string; minApp?: string | null } | null = null
  /** An update asked of this computer from here while it's behind; cleared once its version changes. */
  hostUpdate: HostUpdate | null = null
  bots = new Map<string, Bot>()
  entries = new Map<string, Entry[]>()
  usage: Usage = { providers: [] }
  screen: ScreenState | null = null
  accessRequests: AccessRequest[] = []
  cloud: CloudStatus | null = null
  /** This computer's network; null until known, and on hosts without `networkView`. */
  network: NetworkView | null = null
  /** A new network's recovery code until the user says they saved it: the host keeps it nowhere. */
  recoveryCode: RecoveryCode | null = null
  historyComplete = new Set<string>()
  readonly composerDrafts: ComposerDrafts
  lastError: string | null = null
  /** A kit view selected a bot on this store (new bot or group, editor); the account picks it up. */
  selection: string | null = null
  /** Permission cards whose answer is on its way, with the chosen option. */
  answering = new Map<string, string>()

  onRosterChanged: (() => void) | null = null
  onComputerChanged: ((c: Computer) => void) | null = null

  protected waiting = 0
  protected rev = 0
  protected hostId: string | null = null
  protected retired = false
  protected unreadable = false
  protected saveTimer: ReturnType<typeof setTimeout> | null = null
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
      () => this.setError("Couldn't save the draft on this device. Your text is still here; keep this window open until you send or copy it."))
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

  /** What the update notice offers while this computer is behind this app. */
  get hostUpdateOffer(): HostUpdateOffer {
    return offerHostUpdate(this.hostUpdate, this.hello?.canUpdate === true)
  }

  /** This computer's Codync row at `now` (Settings > Computers); null until its `hello` arrives. */
  hostVersionLine(now = Date.now()): HostVersionLine | null {
    if (!this.hello) return null
    return hostVersionLine(this.hello.version, this.hello.latestVersion, this.mismatch,
      offerHostUpdate(this.hostUpdate, this.hello.canUpdate === true, now), this.hostName, this.hello.os)
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

  get hiddenBots(): Bot[] {
    return [...this.bots.values()].filter((b) => b.hidden).sort((a, b) => a.name.localeCompare(b.name))
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
    return this.entries.get(botId) ?? []
  }

  canLoadOlder(botId: string) {
    return canLoadHistory(this.allEntries(botId), this.historyComplete.has(botId))
  }

  members(group: Bot) {
    return group.members.map((id) => this.bots.get(id)).filter((b): b is Bot => !!b)
  }

  /** A bot's name; `fallback` for one that isn't this computer's (a bot on another computer). */
  authorName(id: string | null | undefined, fallback?: string) {
    return (id && this.bots.get(id)?.name) || fallback || 'A deleted bot'
  }

  get connectionLabel() {
    switch (this.shownConnection.kind) {
      case 'online':
        return this.mismatch ? 'Needs update' : 'Connected'
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

  setError(message: string | null) {
    this.lastError = message
    this.changed()
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
      this.bots = new Map(cache.bots.map((b) => [b.id, b]))
      const grouped = new Map<string, Entry[]>()
      for (const e of cache.entries) grouped.set(e.botId, [...(grouped.get(e.botId) ?? []), e])
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
    const cache = { stamp: `${this.appVersion}/${this.hello?.version ?? ''}`, hostId: this.hostId, rev: this.rev, bots, entries: kept }
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
      return 'Custom agent'
  }
}
