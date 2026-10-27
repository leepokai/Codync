import type { ArtifactCard } from './artifacts'
import type { CallLine } from './calls'
import type { ContextUsage } from './context-usage'

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
  /** The model's own settings (effort, fast…): agent option id → value. */
  modelOptions: Record<string, string>
  /** The agent's Fast mode (Claude Code, Codex): faster replies for more usage. */
  fastMode: boolean
  /** What the agent says about Fast mode: `unknown` (no session yet), `offered`, `unsupported`,
   *  or `refused` (it turned it off; `fastModeIssue` says why). */
  fastModeSupport: string
  fastModeIssue: string | null
  /** Agents that carry on, in order, when the bot's own can't answer. */
  fallbacks: Fallback[]
  /** The bot may set its own fallbacks; null from an older host (no fallbacks there). */
  selfFallbacks: boolean | null
  /** The fallback the bot is on right now; null on its own agent. */
  agentStep: AgentStep | null
  /** Agents of its chain (its own included) resting now; one whose `until` passed isn't. */
  resting: Resting[]
  pinned: boolean
  /** The computer's lead bot, which the host makes: first in the roster, never deleted, hidden or pinned. */
  lead: boolean
  /** Where the user dragged it, lowest first within its band (`rosterBand`); ties sort by recent activity. */
  position: number
  hidden: boolean
  notify?: boolean | null
  connectors: string[]
  skills: string[]
  /** The bot can see and operate the computer's desktop. */
  computer: boolean
  /** The bot can create bots (which can too) and change any bot's setup (the built-in `manage` tools); null from an older host. */
  manageTeam: boolean | null
  /** Other computers in the user's networks list this bot and may message it; null from an older host. */
  offeredToNetwork: boolean | null
  /** The user's cloud assistant lists this bot and may message it; null from an older host. */
  reachableByAssistants: boolean | null
  /** The bot that created this one; null when the user did. */
  createdBy: string | null
  /** Uses the memory all the user's bots share ("About you"). */
  sharedMemory: boolean
  /** Keeps the harness's own memory (Claude Code's…) on beside Codync's; null where it has no switch. */
  harnessMemory: boolean | null
  /** The harness keeps one memory for all its sessions that Codync can't turn off (Hermes Agent, Gemini CLI): every bot on it shares it. */
  harnessMemoryShared: boolean
  /** Moves whenever memory changes, so an open memory screen fetches it again. */
  memoryRev: number
  autoName: boolean
  createdAt: number
  rev: number
  status: BotStatus | string
  activity: string
  startedAt?: number | null
  contextUsage?: ContextUsage | null
  workingChat?: string | null
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
    modelOptions: raw.modelOptions ?? {},
    fastMode: raw.fastMode ?? false,
    fastModeSupport: raw.fastModeSupport ?? 'unknown',
    fastModeIssue: raw.fastModeIssue ?? null,
    fallbacks: raw.fallbacks ?? [],
    selfFallbacks: raw.selfFallbacks ?? null,
    agentStep: raw.agentStep ?? null,
    resting: raw.resting ?? [],
    pinned: raw.pinned ?? false,
    lead: raw.lead ?? false,
    position: raw.position ?? 0,
    hidden: raw.hidden ?? false,
    notify: raw.notify ?? null,
    connectors: raw.connectors ?? [],
    skills: raw.skills ?? [],
    computer: raw.computer ?? false,
    manageTeam: raw.manageTeam ?? null,
    offeredToNetwork: raw.offeredToNetwork ?? null,
    reachableByAssistants: raw.reachableByAssistants ?? null,
    createdBy: raw.createdBy ?? null,
    sharedMemory: raw.sharedMemory ?? raw.managedWorkspace ?? false,
    harnessMemory: raw.harnessMemory ?? null,
    harnessMemoryShared: raw.harnessMemoryShared ?? false,
    memoryRev: raw.memoryRev ?? 0,
    autoName: raw.autoName ?? false,
    createdAt,
    rev: raw.rev ?? 0,
    status: raw.status ?? 'idle',
    activity: raw.activity ?? '',
    startedAt: raw.startedAt ?? null,
    workingChat: raw.workingChat ?? null,
    contextUsage: raw.contextUsage ?? null,
    unread: raw.unread ?? 0,
    lastMessage: raw.lastMessage ?? null,
    lastAt: raw.lastAt ?? createdAt,
  }
}

/** One step of a bot's fallback chain: another agent and the model to use on it. */
export interface Fallback {
  backend: string
  /** A custom command, only from the bot itself (the apps don't offer one). */
  command?: string
  model: string | null
  modelOptions: Record<string, string>
}

/** Why an agent can't answer: usageLimit · outOfCredits · rateLimited · overloaded · signedOut · notStarting · modelMissing */
export type FallbackReason = string

/** The fallback a bot runs on: which step, its agent and model, since when, and why it left the one before. */
export interface AgentStep {
  step: number
  agent: string
  model: string | null
  since: number
  reason: FallbackReason | null
}

/** An agent that can't answer until `until`. */
export interface Resting {
  agent: string
  until: number
  reason: FallbackReason
}

export const isGroup = (b: Bot) => b.kind === 'group'
export const isWorking = (b: Bot) => b.status === 'working' || b.status === 'needsInput'
export const needsInput = (b: Bot) => b.status === 'needsInput'
/** Working right now in this chat (its own, or a group it's in). */
export const isWorkingIn = (b: Bot, chat: string) => isWorking(b) && (b.workingChat ?? b.id) === chat
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

/** One bot's emoji on a message. */
export interface BotReaction {
  bot: string
  emoji: string
}

/** The bots' reactions on one message, by emoji in the order each first appeared. */
export function groupBotReactions(reactions: BotReaction[]): { emoji: string; bots: string[] }[] {
  const groups: { emoji: string; bots: string[] }[] = []
  for (const r of reactions) {
    const group = groups.find((g) => g.emoji === r.emoji)
    if (group) group.bots.push(r.bot)
    else groups.push({ emoji: r.emoji, bots: [r.bot] })
  }
  return groups
}

/** The message a user message replies to, as the host quoted it. */
export interface ReplyQuote {
  id: string
  /** `user`, or the bot that wrote it. */
  author: string
  /** Its text (or file names), at most 500 characters. */
  text: string
  /** The artifact, when it quotes an artifact card: the message is about that artifact. */
  artifact?: string
}

export interface Attachment {
  id: string
  name: string
  size: number
}

export interface SharedFile extends Attachment { sha256: string }

export const isImageName = (name: string) =>
  ['png', 'jpg', 'jpeg', 'heic', 'gif', 'webp', 'tiff', 'bmp'].includes(name.split('.').pop()?.toLowerCase() ?? '')

/** A shared image small enough to show in the chat (the whole file is fetched for it). */
export const showsAsImage = (file: SharedFile) => isImageName(file.name) && file.size <= 20 * 1024 * 1024

export interface ConnectionRequest {
  [key: string]: unknown
}

export interface BotMessage {
  sourceBotId: string
  targetBotId: string
  text: string
  reply?: string
  detail?: string
  /** The peer's name when it is on another of the user's computers (its id is `name@computer`). */
  peerName?: string
}

/** "Earlier context was summarized": the harness's summary, when it gave one. */
export interface Compaction {
  summary?: string | null
  /** The host started it ahead of time; otherwise the harness did when its context filled. */
  byHost?: boolean
}

/** What a new conversation carries when the bot's own chat starts over; editable until the next message sends it. */
export interface HandoffCard {
  /** draft · sent · kept */
  status: string
  /** cold · away · full */
  reason?: string
  text?: string
}

/** notice: another agent of the bot's chain carried on, or (`back`) the bot's own is back. */
export interface AgentSwitch {
  from: string
  to: string
  back: boolean
  reason: FallbackReason | null
  resetsAt: number | null
  /** What the new agent was told, unfolded under the line. */
  carried: string | null
}

export interface EntryData {
  /** Structured conversation data added to the existing team-tool notice. */
  botMessage?: BotMessage
  /** notice: the agent summarized its context; kept out of the chat. */
  compaction?: Compaction
  /** notice: the bot's chat starts over with this handoff (see `HandoffNotice`). */
  handoff?: HandoffCard
  agentSwitch?: AgentSwitch
  /** notice (error): the bot has no fallbacks and its agent ran out; offers to add one. */
  addFallback?: boolean
  heading?: string
  delegationId?: string
  sourceBotId?: string
  targetBotId?: string
  connectionRequest?: ConnectionRequest
  /** A memory line an older host put in the chat. Memory lives in the bot's memory settings, so it isn't shown. */
  memory?: unknown
  memoryFile?: unknown
  routineId?: string
  runId?: string
  /** notice: a failed routine run the user can run again (`retryRoutineRun`). */
  retry?: boolean
  text?: string
  final?: boolean
  /** user: queued | sent | cancelled | failed · tool: pending | in_progress | completed | failed · permission: pending | answered | cancelled | expired */
  status?: string
  clientNonce?: string
  title?: string
  toolKind?: string
  /** tool: whether it's the bot's own bookkeeping (composing its message, keeping its memory), left out of the
   * working steps; a host older than 2.25.0 leaves it out, and the steps then leave out the call too. */
  bookkeeping?: boolean
  output?: string
  diffs?: FileDiff[]
  locations?: { path: string; line?: number }[]
  options?: PermissionOption[]
  selected?: string
  command?: string
  detail?: string
  cwd?: string
  entries?: PlanItem[]
  /** notice: info | error */
  style?: string
  author?: string
  /** user: the message this one replies to. */
  replyTo?: ReplyQuote
  reactions?: string[]
  /** The bots' reactions, one per bot, oldest first. */
  botReactions?: BotReaction[]
  attachments?: Attachment[]
  files?: SharedFile[]
  /** agent: an artifact the bot published (or updated) in this turn. */
  artifact?: ArtifactCard
  callSeconds?: number
  /** The voice call it was said in (user), answered during (agent), or that it ends (notice): the chat shows those
   * messages only in the call's conversation. */
  call?: string
  /** user: `voice` when a call's voice wrote it for the user (`send_to_bot`). */
  from?: string
  /** notice: what was said on a realtime call, oldest first. */
  transcript?: CallLine[]
}

export interface Entry {
  id: string
  seq: number
  botId: string
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
      return true
    case 'notice':
      // Only a fresh conversation gets a line: memory lives in the bot's memory settings, a summary keeps the same
      // conversation going, and a kept handoff changed nothing.
      return !e.data.memory && !e.data.memoryFile && !e.data.compaction && e.data.handoff?.status !== 'kept'
    case 'agent':
      return e.data.final === true
    default:
      return false
  }
}

/** Messages a reply can quote: the user's and the bots' final replies, once the computer has them. */
export function isReplyable(e: Entry): boolean {
  return !e.id.startsWith('local-') && (e.kind === 'user' || (e.kind === 'agent' && e.data.final === true))
}

/** The quote a sending reply shows until the computer's own replaces it. */
export function quoteOf(e: Entry): ReplyQuote {
  const names = [...(e.data.attachments ?? []), ...(e.data.files ?? [])].map((f) => f.name).join(', ')
  const quote: ReplyQuote = { id: e.id, author: e.kind === 'user' ? 'user' : (e.data.author ?? e.botId), text: (e.data.text || names).slice(0, 500) }
  if (e.data.artifact) quote.artifact = e.data.artifact.id
  return quote
}

export interface UsageWindow {
  id: string
  label: string
  percent: number
  resetsAt?: number | null
  resetsText?: string | null
  /** A reading that isn't a share of a cap. Shown instead of the percent, and never the tightest limit. */
  text?: string | null
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
  /** Only for an agent that runs on Node.js, on a computer without its own: Codync's copy is missing or added. */
  node?: 'missing' | 'added' | null
}

export interface ScreenDisplay {
  id: number
  name: string
  width: number
  height: number
  main: boolean
}

export interface ScreenState {
  /** The helper's display name in operating-system permission settings. */
  permissionApp?: string
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
    permissionApp: raw.permissionApp,
    capture: raw.capture ?? false,
    input: raw.input ?? false,
    displays: raw.displays ?? [],
    userControl: raw.userControl ?? false,
    agentBot: raw.agentBot ?? null,
    computerUse: raw.computerUse ?? false,
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
  /** `installHostUpdate` can update this computer (absent before hosts could be updated from a client). */
  canUpdate?: boolean
  /** The latest release the computer found; null while unknown (development builds, Homebrew, Windows; absent from older hosts). */
  latestVersion?: string | null
  cloud?: string | null
  /** Product analytics on this computer: on unless its owner turned it off. */
  analytics?: boolean
}

export interface PairingInfo {
  pairingUrl: string
  urls: string[]
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
  /** A private relay's registration token is saved (the token itself never leaves the host). */
  tokenSet?: boolean | null
  registered?: boolean | null
  connected?: boolean | null
  owner?: CloudOwner | null
  lastError?: string | null
  approval?: AccountApproval | null
  /** Whose relay: a `private` one has no account, access requests or relayed screen. Absent from older hosts (always Codync's). */
  relay?: 'codync' | 'private' | null
  /** The user's cloud assistant may message this computer's bots (`setAssistants`). */
  assistants?: boolean | null
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
  modelOptions?: Record<string, string>
  fastMode?: boolean
  fallbacks?: Fallback[]
  selfFallbacks?: boolean
  pinned?: boolean
  hidden?: boolean
  notify?: boolean | null
  connectors?: string[]
  skills?: string[]
  computer?: boolean
  manageTeam?: boolean
  offeredToNetwork?: boolean
  reachableByAssistants?: boolean
  sharedMemory?: boolean
  harnessMemory?: boolean
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
    modelOptions: bot.modelOptions,
    fastMode: bot.fastMode,
    fallbacks: bot.fallbacks,
    selfFallbacks: bot.selfFallbacks ?? undefined,
    pinned: bot.pinned,
    hidden: bot.hidden,
    notify: bot.notify ?? null,
    connectors: bot.connectors,
    skills: bot.skills,
    computer: bot.computer,
    manageTeam: bot.manageTeam ?? undefined,
    offeredToNetwork: bot.offeredToNetwork ?? undefined,
    reachableByAssistants: bot.reachableByAssistants ?? undefined,
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
