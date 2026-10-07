// Wire types for codync-host (see host/src/api/). Field names match the
// host's camelCase JSON. Timestamps are epoch milliseconds.

export type BotStatus = 'idle' | 'working' | 'needsInput' | 'error'

export interface Bot {
  id: string
  /** `agent`, or `group`: several bots and the user in one chat. */
  kind: string
  members: string[]
  name: string
  description: string
  avatarColor: string
  avatarShape: string
  backend: string
  command?: string | null
  cwd: string
  /** Host-allocated personal workspace rather than a user-selected project. */
  managedWorkspace: boolean
  /** `ask` or `auto`. */
  permission: string
  model?: string | null
  pinned: boolean
  hidden: boolean
  notify?: boolean | null
  connectors: string[]
  skills: string[]
  /** The bot can see and operate the computer's desktop. */
  computer: boolean
  autoName: boolean
  createdAt: number
  rev: number
  status: BotStatus | string
  activity: string
  startedAt?: number | null
  workingChat?: string | null
  workingThread?: string | null
  unread: number
  lastMessage?: string | null
  lastAt: number
}

/** Lenient: a host that is a little newer or older than the app still yields a usable bot. */
export function normalizeBot(raw: Partial<Bot> & { id: string }): Bot {
  const createdAt = raw.createdAt ?? 0
  return {
    id: raw.id,
    kind: raw.kind ?? 'agent',
    members: raw.members ?? [],
    name: raw.name ?? 'Bot',
    description: raw.description ?? '',
    avatarColor: raw.avatarColor ?? 'blue',
    avatarShape: raw.avatarShape ?? 'blob',
    backend: raw.backend ?? 'custom',
    command: raw.command ?? null,
    cwd: raw.cwd ?? '',
    managedWorkspace: raw.managedWorkspace ?? false,
    permission: raw.permission ?? 'auto',
    model: raw.model ?? null,
    pinned: raw.pinned ?? false,
    hidden: raw.hidden ?? false,
    notify: raw.notify ?? null,
    connectors: raw.connectors ?? [],
    skills: raw.skills ?? [],
    computer: raw.computer ?? false,
    autoName: raw.autoName ?? false,
    createdAt,
    rev: raw.rev ?? 0,
    status: raw.status ?? 'idle',
    activity: raw.activity ?? '',
    startedAt: raw.startedAt ?? null,
    workingChat: raw.workingChat ?? null,
    workingThread: raw.workingThread ?? null,
    unread: raw.unread ?? 0,
    lastMessage: raw.lastMessage ?? null,
    lastAt: raw.lastAt ?? createdAt,
  }
}

export const isGroup = (b: Bot) => b.kind === 'group'
export const isWorking = (b: Bot) => b.status === 'working' || b.status === 'needsInput'
export const needsInput = (b: Bot) => b.status === 'needsInput'
/** Working right now in this chat's main conversation (`thread == null`) or in that thread. */
export const isWorkingIn = (b: Bot, chat: string, thread: string | null | undefined) =>
  isWorking(b) && (b.workingChat ?? b.id) === chat && (b.workingThread ?? null) === (thread ?? null)
export const folderName = (b: Bot) =>
  b.managedWorkspace ? 'Personal workspace' : (b.cwd.split('/').filter(Boolean).pop() ?? b.cwd)

export interface FileDiff {
  path: string
  added: number
  removed: number
  isNew: boolean
  startLine: number
  patch: string
}

export interface PermissionOption {
  optionId: string
  name: string
  /** allow_once | allow_always | reject_once | reject_always */
  kind: string
}

export interface PlanItem {
  content: string
  status: string
  priority?: string
}

export interface ThreadSummary {
  count: number
  lastAt: number
  /** Who replied, first reply first: bot ids, and `user`. */
  authors: string[]
  unread?: number
}

export interface Attachment {
  id: string
  name: string
  size: number
}

export const isImageName = (name: string) =>
  ['png', 'jpg', 'jpeg', 'heic', 'gif', 'webp', 'tiff', 'bmp'].includes(name.split('.').pop()?.toLowerCase() ?? '')

export interface ConnectionRequest {
  [key: string]: unknown
}

export interface EntryData {
  connectionRequest?: ConnectionRequest
  routineId?: string
  runId?: string
  text?: string
  final?: boolean
  /** user: queued | sent | cancelled | failed · tool: pending | in_progress | completed | failed · permission: pending | answered | cancelled | expired */
  status?: string
  clientNonce?: string
  title?: string
  toolKind?: string
  output?: string
  diffs?: FileDiff[]
  locations?: { path: string; line?: number }[]
  options?: PermissionOption[]
  selected?: string
  command?: string
  detail?: string
  cwd?: string
  entries?: PlanItem[]
  /** notice: info | error | divider */
  style?: string
  author?: string
  thread?: ThreadSummary
  reactions?: string[]
  attachments?: Attachment[]
  callSeconds?: number
}

export interface Entry {
  id: string
  seq: number
  botId: string
  threadId?: string | null
  rev: number
  /** user | agent | thought | tool | plan | permission | notice */
  kind: string
  turn: number
  data: EntryData
  createdAt: number
  updatedAt: number
}

/** Chat-visible (Grok Bot model): user messages, final replies, approval cards, notices. */
export function isChat(e: Entry): boolean {
  switch (e.kind) {
    case 'user':
    case 'permission':
    case 'notice':
      return true
    case 'agent':
      return e.data.final === true
    default:
      return false
  }
}

export interface UsageWindow {
  id: string
  label: string
  percent: number
  resetsAt?: number | null
  resetsText?: string | null
}

export interface UsageProvider {
  id: string
  name: string
  windows: UsageWindow[]
  source: string
  updatedAt: number
}

export interface Usage {
  providers: UsageProvider[]
}

export interface Backend {
  id: string
  name: string
  available: boolean
  command: string
  installHint: string
  registry?: string
  description?: string
  installed?: boolean
  curated?: boolean
  signedIn?: boolean
  canInstall?: boolean
}

export interface ScreenDisplay {
  id: number
  name: string
  width: number
  height: number
  main: boolean
}

export interface ScreenState {
  enabled: boolean
  connected: boolean
  platform: string
  capture: boolean
  input: boolean
  displays: ScreenDisplay[]
  userControl: boolean
  agentBot?: string | null
  /** Bots with computer use can act here (Windows: always; elsewhere Remote screen is on). */
  computerUse: boolean
  viewers: number
}

export function normalizeScreen(raw: Partial<ScreenState> | undefined | null): ScreenState | null {
  if (!raw) return null
  return {
    enabled: raw.enabled ?? false,
    connected: raw.connected ?? false,
    platform: raw.platform ?? '',
    capture: raw.capture ?? false,
    input: raw.input ?? false,
    displays: raw.displays ?? [],
    userControl: raw.userControl ?? false,
    agentBot: raw.agentBot ?? null,
    computerUse: raw.computerUse ?? raw.enabled ?? false,
    viewers: raw.viewers ?? 0,
  }
}

export interface Hello {
  hostId: string
  name: string
  version: string
  os: string
  device?: string
  home?: string
  backends: Backend[]
  rev: number
  screen?: ScreenState
  urls?: string[]
  computerId?: string
  signKey?: string
  boxKey?: string
  minApp?: string
  cloud?: string | null
  /** Product analytics on this computer; null until its owner chose (absent on older hosts). */
  analytics?: boolean | null
}

/** Events only the desktop app knows about; the host records everything else from API calls. */
export type AnalyticsEvent = 'app_opened' | 'onboarding_completed' | 'signed_in' | 'signed_out'

export interface PairingInfo {
  pairingUrl: string
  urls: string[]
  svg?: string
  expiresAt?: number
}

export interface DirItem {
  name: string
  path: string
  isGit: boolean
}

export interface DirListing {
  path: string
  parent?: string | null
  isGit: boolean
  dirs: DirItem[]
}

/** A pending request as the computer shows it (loopback `accessRequests`). */
export interface AccessRequest {
  requestId: string
  deviceKey: string
  deviceName: string
  platform?: string | null
  email?: string | null
  /** The SAS; nil until the phone revealed its nonce (Approve stays disabled). */
  code?: string | null
  createdAt?: number | null
  expiresAt?: number | null
}

/** A device the host authorized (loopback `devices`). */
export interface AuthorizedDevice {
  key: string
  name: string
  platform?: string | null
  /** `local` (QR) or `account`. */
  source: string
  scopes?: string[]
  leaseUntil?: number | null
  createdAt?: number | null
  lastSeenAt?: number | null
  connected?: boolean | null
}

export interface CloudOwner {
  userId: string
  email?: string | null
}

/** `code`: compare the 6-digit code at the computer; `auto`: let account devices in. */
export type AccountApproval = 'code' | 'auto'

export interface CloudStatus {
  enabled: boolean
  url?: string | null
  registered?: boolean | null
  connected?: boolean | null
  owner?: CloudOwner | null
  lastError?: string | null
  approval?: AccountApproval | null
}

export interface ClaimSignature {
  computerId: string
  signKey: string
  boxKey: string
  name: string
  platform: string
  device?: string | null
  version: string
  sig: string
}

/** A computer this device talks to. Holds no secrets: only pinned public keys and addresses. */
export interface Computer {
  id: string
  name: string
  signKey: string
  boxKey?: string | null
  urls: string[]
  cloud?: string | null
  device?: string | null
  color?: string | null
}

/** Create / update payload for a bot. */
export interface BotDraft {
  id?: string
  name: string
  description: string
  avatarColor: string
  avatarShape: string
  backend: string
  command?: string | null
  cwd: string
  permission: string
  model?: string | null
  pinned?: boolean
  hidden?: boolean
  notify?: boolean | null
  connectors?: string[]
  skills?: string[]
  computer?: boolean
}

export function draftOf(bot: Bot): BotDraft {
  return {
    id: bot.id,
    name: bot.name,
    description: bot.description,
    avatarColor: bot.avatarColor,
    avatarShape: bot.avatarShape,
    backend: bot.backend,
    command: bot.command ?? null,
    cwd: bot.managedWorkspace ? '' : bot.cwd,
    permission: bot.permission,
    model: bot.model ?? null,
    pinned: bot.pinned,
    hidden: bot.hidden,
    notify: bot.notify ?? null,
    connectors: bot.connectors,
    skills: bot.skills,
    computer: bot.computer,
  }
}

export interface GroupDraft {
  id?: string
  kind: 'group'
  name: string
  description: string
  members: string[]
  pinned?: boolean
}
