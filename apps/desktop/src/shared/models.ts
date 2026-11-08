import type { ApprovalRule } from './approval-rules'
import type { ArtifactCard } from './artifacts'
import type { CallLine } from './calls'
import { isInternalEntry } from './entry-visibility.ts'
export { isInternalEntry } from './entry-visibility.ts'

// Wire types for codync-host (see host/src/api/). Field names match the
// host's camelCase JSON. Timestamps are epoch milliseconds.

export * from './bdots-models.ts'
import {
  DEFAULT_PROACTIVE_SETTINGS,
  type ActionClass,
  type BotApp,
  type ProactiveSettings,
  type RulePolicy,
} from './bdots-models.ts'

export type BotStatus = 'idle' | 'working' | 'needsInput' | 'error'

/** Three layers of a bot's character (see the host's `agent::personality`). */
export interface Personality {
  soul: string
  persona: string
  heartbeat: string
}

export interface Bot {
  id: string
  /** `agent`, or `group`: several bots and the user in one chat. */
  kind: string
  members: string[]
  chiefBotId?: string | null
  customIcon?: import('./custom-icons').CustomIcon
  name: string
  description: string
  avatarColor: string
  avatarShape: string
  avatarLook?: string | null
  backend: string
  command?: string | null
  cwd: string
  /** Host-allocated personal workspace rather than a user-selected project. */
  managedWorkspace: boolean
  /** `ask` or `auto`. */
  permission: string
  /** The agent mode it works in (Claude's Manual / Plan / AutoÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦); null keeps the default. */
  mode: string | null
  section?: string | null
  actionRules?: Partial<Record<ActionClass, RulePolicy>>
  proactive?: ProactiveSettings
  model?: string | null
  /** The model's own settings (effort, fastÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦): agent option id ÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬Ãƒâ€šÃ‚Â ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â‚¬Å¡Ã‚Â¬ÃƒÂ¢Ã¢â‚¬Å¾Ã‚Â¢ value. */
  modelOptions: Record<string, string>
  /** The agent's Fast mode (Claude Code, Codex): faster replies for more usage. */
  fastMode: boolean
  /** What the agent says about Fast mode: `unknown` (no session yet), `offered`, `unsupported`,
   *  or `refused` (it turned it off; `fastModeIssue` says why). */
  fastModeSupport: string
  fastModeIssue: string | null
  account?: string | null
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
  /** The bot can create bots (which can too) and change its own setup and the bots it created (the built-in `manage` tools); null from an older host. */
  manageTeam: boolean | null
  /** A command run in the workspace after each turn: the turn counts as done only if it passes; null when none (or an older host). */
  verifyCommand: string | null
  /** The model routine runs and check-ins use instead of `model` (a cheaper one); null when none (or an older host). */
  backgroundModel: string | null
  /** A named protected-browser profile: its sign-ins and cookies persist. */
  browserProfile: string | null
  /** A hex tint behind this bot's chat. */
  chatColor: string | null
  /** Saver: once its daily token budget reaches 80 percent, new sessions start on the background model until tomorrow. */
  saver: boolean
  /** The user's rules for the agent's permission requests, first match first. */
  approvalRules: ApprovalRule[]
  /** Whether the quick rules (`actionRules`) are enforced; null from an older host. */
  actionRulesOn: boolean | null
  /** The security profile it follows (readOnly, builder, operator, full), until its rules are changed away from it. */
  securityProfile: string | null
  /** The bot's character, written by the user; empty layers when none (or an older host). */
  personality: Personality
  /** Other computers in the user's networks list this bot and may message it; null from an older host. */
  offeredToNetwork: boolean | null
  /** The user's cloud assistant lists this bot and may message it; null from an older host. */
  reachableByAssistants: boolean | null
  /** The bot that created this one; null when the user did. */
  createdBy: string | null
  computerPermissions?: { browser: boolean; files: boolean; shell: boolean }
  /** Uses the memory all the user's bots share ("About you"). */
  sharedMemory: boolean
  /** Keeps the harness's own memory (Claude Code'sÃƒÆ’Ã†â€™Ãƒâ€šÃ‚Â¢ÃƒÆ’Ã‚Â¢ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡Ãƒâ€šÃ‚Â¬ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â¦) on beside Codync's; null where it has no switch. */
  harnessMemory: boolean | null
  /** The harness keeps one memory for all its sessions that Codync can't turn off (Hermes Agent, Gemini CLI): every bot on it shares it. */
  harnessMemoryShared: boolean
  /** Moves whenever memory changes, so an open memory screen fetches it again. */
  memoryRev: number
  /** Chief of Staff coordinator role (INIC-04). */
  isChief: boolean
  autoName: boolean
  createdAt: number
  rev: number
  status: BotStatus | string
  activity: string
  startedAt?: number | null
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
    avatarLook: raw.avatarLook ?? null,
    backend: raw.backend ?? 'custom',
    command: raw.command ?? null,
    cwd: raw.cwd ?? '',
    managedWorkspace: raw.managedWorkspace ?? false,
    permission: raw.permission ?? 'auto',
    mode: raw.mode ?? null,
    section: raw.section || undefined,
    actionRules: raw.actionRules ?? {},
    proactive: { ...DEFAULT_PROACTIVE_SETTINGS, ...(raw.proactive ?? {}) },
    model: raw.model ?? null,
    modelOptions: raw.modelOptions ?? {},
    fastMode: raw.fastMode ?? false,
    fastModeSupport: raw.fastModeSupport ?? 'unknown',
    fastModeIssue: raw.fastModeIssue ?? null,
    account: raw.account ?? null,
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
    verifyCommand: raw.verifyCommand ?? null,
    backgroundModel: raw.backgroundModel ?? null,
    browserProfile: raw.browserProfile ?? null,
    chatColor: raw.chatColor ?? null,
    saver: raw.saver ?? false,
    approvalRules: raw.approvalRules ?? [],
    actionRulesOn: raw.actionRulesOn ?? null,
    securityProfile: raw.securityProfile ?? null,
    personality: { soul: '', persona: '', heartbeat: '', ...raw.personality },
    offeredToNetwork: raw.offeredToNetwork ?? null,
    reachableByAssistants: raw.reachableByAssistants ?? null,
    createdBy: raw.createdBy ?? null,
    computerPermissions: raw.computerPermissions ?? { browser: false, files: false, shell: false },
    sharedMemory: raw.sharedMemory ?? raw.managedWorkspace ?? false,
    harnessMemory: raw.harnessMemory ?? null,
    harnessMemoryShared: raw.harnessMemoryShared ?? false,
    memoryRev: raw.memoryRev ?? 0,
    isChief: raw.isChief ?? false,
    chiefBotId: raw.chiefBotId ?? null,
    autoName: raw.autoName ?? false,
    createdAt,
    rev: raw.rev ?? 0,
    status: raw.status ?? 'idle',
    activity: raw.activity ?? '',
    startedAt: raw.startedAt ?? null,
    workingChat: raw.workingChat ?? null,
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

/** Why an agent can't answer: usageLimit ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· outOfCredits ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· rateLimited ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· overloaded ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· signedOut ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· notStarting ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· modelMissing */
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
  b.managedWorkspace ? 'Personal workspace' : (b.cwd.split(/[\\/]/).filter(Boolean).pop() ?? b.cwd)

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
  /** draft ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· sent ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· kept */
  status: string
  /** cold ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· away ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· full */
  reason?: string
  text?: string
}

export interface PasskeyWallCard {
  kind: 'webauthn' | 'totp' | 'captcha' | 'login'
  url: string
  reason: string
  tabId?: string
  status?: 'waiting' | 'resolved' | 'cancelled'
}

export interface ChiefIncidentCard {
  chiefBotId: string
  workerBotId: string
  reason: 'stall' | 'failed' | 'timeout' | 'loop'
  retryAttempt: number
  summary: string
  status?: 'retrying' | 'escalated' | 'resolved'
}

export interface SentinelIssue {
  signature: string
  kind: 'tool_error' | 'retry_loop' | 'turn_error' | 'queue_lag'
  severity: 'low' | 'medium' | 'high'
  title: string
  toolName?: string
  botId: string
  firstSeenAt: number
  lastSeenAt: number
  count: number
  wastedTurns: number
  evidenceEntryId: string
}

export interface SentinelReport {
  scannedAt: number
  lookbackMs: number
  entriesExamined: number
  issues: SentinelIssue[]
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
  /** Internal scheduled input or provisional background output; never rendered. */
  internal?: boolean
  /** An "I learned" notice: what its Undo needs (`undoMemoryChange`); `undone` once taken back. */
  memoryUndo?: { file: string; shared: boolean; before: number | null; change: number; undone: boolean }
  source?: string
  /** Structured conversation data added to the existing team-tool notice. */
  botMessage?: BotMessage
  /** notice: the agent summarized its context; kept out of the chat. */
  compaction?: Compaction
  /** notice: the bot's chat starts over with this handoff (see `HandoffNotice`). */
  handoff?: HandoffCard
  /** A routine root line that owns a thread. */
  routineThread?: string
  /** A bot asking the people of a group to answer (`ask_member`). */
  memberQuestion?: { status: string; answers: string[] }
  /** A message sent on from another chat (the menu's "Send to a channel"). */
  forwardedFrom?: { botId: string; entryId: string; author: string }
  /** notice: a bot-built interactive mini-app card (Idea 1). */
  botApp?: BotApp
  /** notice: human takeover request card (Idea 13). */
  takeoverRequest?: { reason: string; url?: string | null; status: string }
  /** notice: a turn that stopped half done (out of steps, or its answer cut); one tap continues it. */
  pausedTurn?: { verdict: 'paused' | 'truncated'; status: 'waiting' | 'continued'; round: number; canContinue: boolean }
  /** Interactive Passkey / 2FA assistance card from CDP browser detector (INIC-02). */
  passkeyWall?: PasskeyWallCard
  /** Chief of Staff incident intervention card (INIC-04). */
  chiefIncident?: ChiefIncidentCard
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
  /** notice: the computer's current health banner; the host updates it in place. */
  health?: boolean
  /** notice: `warning`, `critical` or `normal`, with `health`. */
  level?: string
  text?: string
  final?: boolean
  /** user: queued | sent | cancelled | failed ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· tool: pending | in_progress | completed | failed ÃƒÆ’Ã†â€™ÃƒÂ¢Ã¢â€šÂ¬Ã…Â¡ÃƒÆ’Ã¢â‚¬Å¡Ãƒâ€šÃ‚Â· permission: pending | answered | cancelled | expired */
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
  voiceTranscriptText?: string
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
  /** The side conversation it's in (a document's `doc:<id>`); absent in the chat itself. */
  threadId?: string | null
  /** On a thread's root line: how many of its replies are unread. */
  threadUnread?: number | null
}

/** Chat-visible (Grok Bot model): user messages, final replies, approval cards, notices. */
export function isChat(e: Entry): boolean {
  if (isInternalEntry(e)) return false
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
  return !isInternalEntry(e) && !e.id.startsWith('local-') && (e.kind === 'user' || (e.kind === 'agent' && e.data.final === true))
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
  status?: 'unsupported' | 'unavailable' | string
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
  /** The signed-in account, when the agent's own files name it. */
  email?: string | null
  /** Only for an agent that runs on Node.js, on a computer without its own: Codync's copy is missing or added. */
  node?: 'missing' | 'added' | null
  /** Whether this agent can keep isolated accounts beside the computer's own sign-in. */
  accounts?: boolean
  /** Whether the computer's own sign-in can be removed (a logout command or its credential files). */
  signOut?: boolean
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

/** The n8n instance the host runs; `url` is set once its folder is. */
export interface N8nState {
  enabled: boolean
  state: 'stopped' | 'starting' | 'running' | 'error'
  error: string | null
  url: string | null
  port: number
  path: string | null
  /** Whether the instance's MCP server is connected to the bots. */
  mcp: boolean
  /** The instance's session cookie: the editor signs in with it. */
  auth: string | null
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
  n8n?: N8nState
  icons?: Record<string, { dataUrl: string; tinted: boolean }>
  iconsRevision?: number
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


/** A signed-in phone that scanned a computer in no account, waiting to be let in (loopback `pairingRequests`). */
export interface PairingRequest {
  requestId: string
  deviceName: string
  platform?: string | null
  /** The phone's account, which the computer joins. */
  email?: string | null
  /** The 6 digits the phone shows too. */
  code: string
  expiresAt?: number | null
}

export interface DirListing {
  path: string
  parent?: string | null
  isGit: boolean
  dirs: DirItem[]
  recent?: DirItem[]
  ancestors?: DirItem[]
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

/** A linked WhatsApp number on this computer. */
export interface WhatsAppState {
  /** disconnected, starting, qr, pairing, connected, loggedOut or error. */
  state: string
  qr?: string | null
  /** The code to type in WhatsApp → Linked devices instead of scanning a QR code. */
  pairCode?: string | null
  me?: string | null
  name?: string | null
  error?: string | null
  /** The bot whose chat receives this number's messages. */
  bot?: string | null
  /** Chats and people allowed to write; empty means anyone. */
  allowed?: string[]
}

export interface CloudStatus {
  canJoinAccount?: boolean
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
  avatarLook?: string | null
  backend: string
  command?: string | null
  cwd: string
  permission: string
  mode?: string | null
  section?: string | null
  actionRules?: Partial<Record<ActionClass, RulePolicy>>
  proactive?: ProactiveSettings
  model?: string | null
  modelOptions?: Record<string, string>
  fastMode?: boolean
  account?: string | null
  fallbacks?: Fallback[]
  selfFallbacks?: boolean
  pinned?: boolean
  hidden?: boolean
  notify?: boolean | null
  connectors?: string[]
  skills?: string[]
  computer?: boolean
  manageTeam?: boolean
  verifyCommand?: string | null
  backgroundModel?: string | null
  /** A named protected-browser profile: its sign-ins and cookies persist. */
  browserProfile?: string | null
  chatColor?: string | null
  saver?: boolean
  approvalRules?: ApprovalRule[]
  actionRulesOn?: boolean
  securityProfile?: string | null
  personality?: Personality
  computerPermissions?: { browser: boolean; files: boolean; shell: boolean }
  offeredToNetwork?: boolean
  reachableByAssistants?: boolean
  sharedMemory?: boolean
  harnessMemory?: boolean
  isChief?: boolean
  chiefBotId?: string | null
  customIcon?: import('./custom-icons').CustomIcon
}

export function draftOf(bot: Bot): BotDraft {
  return {
    id: bot.id,
    name: bot.name,
    description: bot.description,
    avatarColor: bot.avatarColor,
    avatarShape: bot.avatarShape,
    avatarLook: bot.avatarLook ?? null,
    backend: bot.backend,
    command: bot.command ?? null,
    cwd: bot.managedWorkspace ? '' : bot.cwd,
    permission: bot.permission,
    actionRules: bot.actionRules,
    proactive: bot.proactive,
    model: bot.model ?? null,
    modelOptions: bot.modelOptions,
    fastMode: bot.fastMode,
    account: bot.account ?? null,
    fallbacks: bot.fallbacks,
    selfFallbacks: bot.selfFallbacks ?? undefined,
    pinned: bot.pinned,
    hidden: bot.hidden,
    notify: bot.notify ?? null,
    connectors: bot.connectors,
    skills: bot.skills,
    computer: bot.computer,
    manageTeam: bot.manageTeam ?? undefined,
    verifyCommand: bot.verifyCommand,
    backgroundModel: bot.backgroundModel,
    browserProfile: bot.browserProfile,
    chatColor: bot.chatColor,
    saver: bot.saver,
    approvalRules: bot.approvalRules,
    actionRulesOn: bot.actionRulesOn ?? undefined,
    securityProfile: bot.securityProfile,
    personality: bot.personality,
    computerPermissions: bot.computerPermissions ?? { browser: false, files: false, shell: false },
    offeredToNetwork: bot.offeredToNetwork ?? undefined,
    reachableByAssistants: bot.reachableByAssistants ?? undefined,
    isChief: bot.isChief,
    chiefBotId: bot.chiefBotId ?? null,
  }
}

export interface AgentAccount {
  id: string
  name: string
  backend: string
  createdAt: number
  /** The account's sign-in address, when its CLI's files name it. */
  email?: string | null
}

export interface GroupDraft {
  id?: string
  kind: 'group'
  name: string
  description: string
  members: string[]
  chiefBotId?: string | null
  customIcon?: import('./custom-icons').CustomIcon
  pinned?: boolean
}

/** The last days at a glance, counted by the host from this computer's transcripts (`weekSummary`). */
export interface WeekSummary {
  messages: number
  replies: number
  tools: number
  fileEdits: number
  approvalsAsked: number
  approvalsAnswered: number
  approvalsWaiting: number
  activeBots: number
  topBots: { id: string; name: string; entries: number }[]
}
/** One thing `doctor` found wrong with a bot's setup. */
export interface DoctorFinding {
  botId: string
  botName: string
  severity: 'error' | 'warning'
  problem: string
  message: string
}

export interface DoctorReport {
  checked: number
  errors: number
  findings: DoctorFinding[]
}

/** A daily token limit and today's use; `botId` is `*` for all bots together. */
export interface TokenBudget {
  botId: string
  limit: number
  used: number
  level: 'within' | 'warning' | 'exceeded'
}

/** A bot (or bots) as one portable JSON file: the host makes and reads it. */
export type BotBundle = Record<string, unknown>
