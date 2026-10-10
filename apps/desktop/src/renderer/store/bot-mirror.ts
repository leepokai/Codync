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
import type { HostClient, HostRoute, HostTransport } from '../client/host-client'
import { Observable } from '../lib/observable'
import { ComposerDrafts } from '@shared/composer-drafts'
import { canLoadHistory } from '@shared/mirror-window'

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
  computer: Computer
  connection: Connection = { kind: 'connecting' }
  hostRoute: HostRoute | null = null
  client: HostClient | null = null
  hello: Hello | null = null
  hostVersion: { version: string; minApp?: string | null } | null = null
  bots = new Map<string, Bot>()
  entries = new Map<string, Entry[]>()
  usage: Usage = { providers: [] }
  screen: ScreenState | null = null
  accessRequests: AccessRequest[] = []
  cloud: CloudStatus | null = null
  historyComplete = new Set<string>()
  routineDrafts = new Map<string, string>()
  readonly composerDrafts: ComposerDrafts
  lastError: string | null = null
  /** A kit view selected a bot on this store (new chat, editor); the account picks it up. */
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
    protected clientKind = 'mac',
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

  /** Roster order: pinned first, then where the user dragged them, then most recent activity. */
  get roster(): Bot[] {
    return [...this.bots.values()].filter((b) => !b.hidden).sort((a, b) => Number(b.pinned) - Number(a.pinned) || a.position - b.position || b.lastAt - a.lastAt)
  }

  get hiddenBots(): Bot[] {
    return [...this.bots.values()].filter((b) => b.hidden).sort((a, b) => a.name.localeCompare(b.name))
  }

  backendName(id: string) {
    return this.hello?.backends.find((b) => b.id === id)?.name ?? builtInBackendName(id)
  }

  /** A bot's or group's main chat (thread replies are left out). */
  chat(botId: string) {
    return (this.entries.get(botId) ?? []).filter((e) => !e.threadId)
  }

  allEntries(botId: string) {
    return this.entries.get(botId) ?? []
  }

  canLoadOlder(botId: string) {
    return canLoadHistory(this.allEntries(botId), this.historyComplete.has(botId))
  }

  replies(botId: string, root: string) {
    return (this.entries.get(botId) ?? []).filter((e) => e.threadId === root)
  }

  members(group: Bot) {
    return group.members.map((id) => this.bots.get(id)).filter((b): b is Bot => !!b)
  }

  authorName(id: string | null | undefined) {
    return (id && this.bots.get(id)?.name) || 'A deleted bot'
  }

  /** The latest thinking of the turn running in a chat (`thread` null) or thread. */
  currentThinking(botId: string, thread: string | null) {
    const list = this.allEntries(botId)
    for (let i = list.length - 1; i >= 0; i--) {
      const e = list[i]!
      if ((e.threadId ?? null) !== thread) continue
      if (e.kind === 'user' || (e.kind === 'agent' && e.data.final === true)) return null
      if (e.kind === 'thought' && e.data.text) return e.data.text
    }
    return null
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

  // MARK: cache (instant launch, offline reading)

  private get cacheKey() {
    return `codync-mirror-${this.contextId}-${this.computer.id}`
  }

  private loadCache() {
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
    const cache = { stamp: `${this.appVersion}/${this.hello?.version ?? ''}`, hostId: this.hostId, rev: this.rev, bots: [...this.bots.values()], entries: kept }
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
