import type { ArtifactCard } from './artifacts'
import type { CallLine } from './calls'
import { normalizeDevServer, type DevServer } from './dev-servers.ts'
import { normalizeMaxThreads, normalizeOutside, normalizeWork, type OutsideStatus, type WorkTask } from './work.ts'

// Wire types for codync-host (see host/src/api/). Field names match the
// host's camelCase JSON. Timestamps are epoch milliseconds.

export type BotStatus = 'idle' | 'working' | 'needsInput' | 'error'

export interface Bot {
  customIcon?: import('./custom-icons').CustomIcon
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
  /** The login profile it runs on; absent for the login the computer already has. */
  loginProfile?: string | null
  cwd: string
  /** The bot's own folder, which the host made and keeps versions of, rather than a project the user chose. */
  managedWorkspace: boolean
  /** `ask` or `auto`: whether Codync asks about what the agent asks, or approves it. */
  permission: string
  /** The agent mode it works in (Claude's Manual / Plan / Auto…): the agent's id, or `auto` for its Auto mode under
   *  any id. Null keeps the agent's default. */
  mode: string | null
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
  /** Advisor mode: the agent its threads run on, null when they run on the bot's own. */
  threadAgent: Fallback | null
  /** In Advisor mode, the threads' fallbacks; null from an older host (no Advisor mode there). */
  threadFallbacks: Fallback[] | null
  /** The fallback the bot is on right now; null on its own agent. */
  agentStep: AgentStep | null
  /** Agents of its chain (its own included) resting now; one whose `until` passed isn't. */
  resting: Resting[]
  /** What it keeps running (`dev-servers.ts`). */
  devServers: DevServer[]
  pinned: boolean
  /** The computer's lead bot: the host makes the first, and the user may pick another (`setLead`) or delete it. Placed in the roster like any bot, never hidden. */
  lead: boolean
  /** Where the user dragged it, lowest first within its band (`rosterBand`); ties sort by recent activity. */
  position: number
  /** The roster section the user put it in (`Work`, `Personal`); none when absent. */
  section?: string
  hidden: boolean
  /** The computer's box for threads without a bot: an agent on its own. Never a bot to the user. */
  plain: boolean
  /** Paused by the user (or a managing bot): it answers nothing and spends nothing until it is resumed; missing from an older host reads as not paused. */
  paused: boolean
  notify?: boolean | null
  connectors: string[]
  skills: string[]
  /** The `/` commands of the agent it runs on now, as the agent lists them; none for a group. */
  agentCommands: AgentCommand[]
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
  /** When Codync summarizes the chat ahead of time; null where it doesn't (an agent it can't ask, an older host). */
  compaction: CompactionThreshold | null
  /** The sizes the chat's window allows; null until its agent has told the window. */
  compactionTokens: CompactionSizes | null
  /** Moves whenever memory changes, so an open memory screen fetches it again. */
  memoryRev: number
  autoName: boolean
  createdAt: number
  rev: number
  status: BotStatus | string
  activity: string
  startedAt?: number | null
  workingChat?: string | null
  /** Its box (`work.ts`): the live and waiting tasks in dispatch order, then those ended in the last day; none from
   * a host before the box. */
  work: WorkTask[]
  /** The most threads it runs at once, 1 to 6; null from a host before the box. */
  maxThreads: number | null
  /** A thread that starts while another of the bot's works in the same git repository gets a worktree of its own;
   * null from a host before worktrees. */
  threadWorktrees: boolean | null
  /** The bot's folder is in a git repository: where `threadWorktrees` matters. */
  gitRepo: boolean
  /** The user's folders it sends work to, the first its default; empty from a host before projects (`hello.projects`),
   * whose `cwd` is still the bot's one folder. */
  projects: string[]
  /** Whether each of `projects`, in the same order, is in a git repository: where `worktreeMode` matters. */
  projectGitRepos: boolean[]
  /** When its threads get a worktree; null from a host before projects (it has `threadWorktrees`). */
  worktreeMode: WorktreeMode | null
  /** Per project (its path), the folders of its main checkout linked into each new worktree of it (`node_modules`);
   * null from a host before them (`hello.worktreeFill`). */
  sharedFolders: Record<string, string[]> | null
  /** Git is on the bot's computer; a host that doesn't say counts as having it. */
  gitInstalled: boolean
  /** How it stands outside its chat (`outsideStatus`); null from a host before it. `status` is the chat's alone. */
  outside: OutsideStatus | null
  unread: number
  /** How many of `unread` are cards waiting on the user: the badge turns amber. 0 from an older host. */
  waiting: number
  lastMessage?: string | null
  lastAt: number
}

/** Lenient: a host that is a little newer or older than the app still yields a usable bot. */
/** The messages in a bot's badge the user hasn't read: what `markRead` clears (waiting cards stay counted). */
export function unseen(bot: Pick<Bot, 'unread' | 'waiting'>): number {
  return bot.unread - bot.waiting
}

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
    loginProfile: raw.loginProfile ?? null,
    cwd: raw.cwd ?? '',
    managedWorkspace: raw.managedWorkspace ?? false,
    permission: raw.permission ?? 'ask',
    mode: raw.mode ?? null,
    model: raw.model ?? null,
    modelOptions: raw.modelOptions ?? {},
    fastMode: raw.fastMode ?? false,
    fastModeSupport: raw.fastModeSupport ?? 'unknown',
    fastModeIssue: raw.fastModeIssue ?? null,
    fallbacks: raw.fallbacks ?? [],
    selfFallbacks: raw.selfFallbacks ?? null,
    threadAgent: raw.threadAgent ?? null,
    threadFallbacks: raw.threadFallbacks ?? null,
    agentStep: raw.agentStep ?? null,
    agentCommands: raw.agentCommands ?? [],
    resting: raw.resting ?? [],
    devServers: (raw.devServers ?? []).map(normalizeDevServer),
    pinned: raw.pinned ?? false,
    lead: raw.lead ?? false,
    position: raw.position ?? 0,
    // A cleared section (null, or an empty name) reads as none.
    section: raw.section || undefined,
    hidden: raw.hidden ?? false,
    plain: raw.plain === true,
    // Only a boolean true. A missing flag, or anything else an older host sent, is not paused.
    paused: raw.paused === true,
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
    compaction: raw.compaction ?? null,
    compactionTokens: raw.compactionTokens ?? null,
    memoryRev: raw.memoryRev ?? 0,
    autoName: raw.autoName ?? false,
    createdAt,
    rev: raw.rev ?? 0,
    status: raw.status ?? 'idle',
    activity: raw.activity ?? '',
    startedAt: raw.startedAt ?? null,
    workingChat: raw.workingChat ?? null,
    work: normalizeWork(raw.work),
    maxThreads: normalizeMaxThreads(raw.maxThreads),
    threadWorktrees: raw.threadWorktrees ?? null,
    gitRepo: raw.gitRepo ?? false,
    projects: raw.projects ?? [],
    projectGitRepos: raw.projectGitRepos ?? [],
    worktreeMode: raw.worktreeMode ?? null,
    sharedFolders: raw.sharedFolders ?? null,
    gitInstalled: raw.gitInstalled ?? true,
    outside: normalizeOutside(raw.outside),
    unread: raw.unread ?? 0,
    waiting: raw.waiting ?? 0,
    lastMessage: raw.lastMessage ?? null,
    lastAt: raw.lastAt ?? createdAt,
  }
}

/** One step of a bot's fallback chain: another agent and the model to use on it. */
export interface Fallback {
  backend: string
  /** A custom command, only from the bot itself (the apps don't offer one). */
  command?: string
  /** The login profile the step runs on; absent for the login the computer already has. */
  loginProfile?: string | null
  model: string | null
  modelOptions: Record<string, string>
}

/** Why an agent can't answer: usageLimit · outOfCredits · rateLimited · overloaded · signedOut · notStarting · modelMissing · modeMissing */
export type FallbackReason = string

/** The fallback a bot runs on: which step, its agent and model, since when, and why it left the one before. */
/** One of an agent's own `/` commands (ACP `available_commands_update`). */
export interface AgentCommand {
  /** What follows the `/`. */
  name: string
  description: string
  /** What to type after the name, for a command that takes input. */
  hint?: string
}

export interface AgentStep {
  step: number
  agent: string
  /** The login profile the step runs on; absent for the computer's own login. */
  loginProfile?: string | null
  model: string | null
  since: number
  reason: FallbackReason | null
}

/** An agent that can't answer until `until`. */
export interface Resting {
  agent: string
  /** The login profile that rests; absent for the computer's own login. */
  loginProfile?: string | null
  until: number
  reason: FallbackReason
}

export const isGroup = (b: Bot) => b.kind === 'group'
export const isWorking = (b: Bot) => b.status === 'working' || b.status === 'needsInput'
export const needsInput = (b: Bot) => b.status === 'needsInput'
/** Working right now in this chat (its own, or a group it's in). */
export const isWorkingIn = (b: Bot, chat: string) => isWorking(b) && (b.workingChat ?? b.id) === chat
/** Where the bot works, as a list shows it: its default project's name, else its own folder, or (an older host) its
 * project folder's name. */
export const folderName = (b: Bot, tr: (text: string) => string) => {
  const folder = b.projects[0] ?? (b.managedWorkspace ? null : b.cwd)
  return folder === null ? tr('Own folder') : (folder.split(/[\\/]/).filter(Boolean).pop() ?? folder)
}

/** When a bot's threads get a git worktree of their own, in a project that is a git repository. */
export type WorktreeMode = 'off' | 'sideBySide' | 'always'

/** A subagent, as its launching call reports it: the totals once it finished in the foreground. */
export interface Subagent {
  kind?: string
  model?: string
  background?: boolean
  tokens?: number
  toolUses?: number
  durationMs?: number
}

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
  /** Its text (or file names), at most 500 characters; the part the user selected in it when `part` is set. */
  text: string
  /** The reply answers the part of the message the user selected, not all of it. */
  part?: boolean
  /** The artifact, when it quotes an artifact card: the message is about that artifact. */
  artifact?: string
}

export interface Attachment {
  id: string
  name: string
  size: number
}

export interface SharedFile extends Attachment {
  sha256: string
  /** A sound the chat plays in place, as the host stamps it; missing (an older host) means a plain file card. */
  playsInChat?: boolean
}

export const isImageName = (name: string) =>
  ['png', 'jpg', 'jpeg', 'heic', 'gif', 'webp', 'tiff', 'bmp'].includes(name.split('.').pop()?.toLowerCase() ?? '')

export const isAudioName = (name: string) => ['mp3', 'm4a', 'wav', 'aac'].includes(name.split('.').pop()?.toLowerCase() ?? '')

/** The largest shared image a chat fetches whole to show. */
const INLINE_BYTES = 20 * 1024 * 1024

/** A shared image small enough to show in the chat. */
export const showsAsImage = (file: SharedFile) => isImageName(file.name) && file.size <= INLINE_BYTES


/** The voice note playing at the top of a message's bubble; none when its files show as their own cards. */
export const voiceNoteOf = (data: EntryData) => {
  const file = data.voiceNote ? data.files?.[0] : undefined
  return file?.playsInChat ? file : undefined
}

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

/** When Codync summarizes a bot's chat ahead of time: a preset, or at a size in tokens. */
export type CompactionThreshold = 'early' | 'nearLimit' | 'off' | number

/** The sizes a chat's window allows: `min`..`max` by `step` tokens, and the few a menu lists. */
export interface CompactionSizes {
  min: number
  max: number
  step: number
  presets: number[]
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
  /** The user made the switch (another agent or login profile): the next message carries the conversation. */
  manual?: boolean
  /** Where a switch the user made is: its next message hasn't gone yet, it carried the conversation, or nothing. */
  state?: 'pending' | 'handedOff' | 'switched'
  /** What the chat calls each side: the agent, with its login profile ("Claude Code (Work)"). */
  fromName?: string
  toName?: string
}

/** A choice card's (`request_decision`) lifecycle, as the host writes it in `status`. */
export type DecisionStatus = 'pending' | 'answered' | 'expired'

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
  /** notice: a bot chain stopped at its handoff limit; Continue (`continueChain`) lets it go on, once. */
  continueChain?: boolean
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
  /** user: taken back from the queue before it went out (`unqueue`); no view reads it (`withoutRemoved`). */
  removed?: boolean
  clientNonce?: string
  title?: string
  toolKind?: string
  /** tool and permission: the agent's id for the call, so an approval finds the tool entry with what it returned. */
  toolCallId?: string
  /** tool: whether it's the bot's own bookkeeping (composing its message, keeping its memory), left out of the
   * working steps; a host older than 2.25.0 leaves it out, and the steps then leave out the call too. */
  bookkeeping?: boolean
  /** tool: the call that launched the subagent making this one (Claude Code only). */
  parent?: string
  /** tool: what a subagent's launch says about it (Claude Code only). */
  subagent?: Subagent
  output?: string
  diffs?: FileDiff[]
  locations?: { path: string; line?: number }[]
  options?: PermissionOption[]
  selected?: string
  /** An additive notice card; older clients still display its text. Its `status` is a `DecisionStatus`. */
  decision?: boolean
  /** What the bot asks the user to choose (`request_decision`). */
  question?: string
  /** decision: the choices, in the order the bot offered them. */
  choices?: string[]
  /** decision: which choice the bot suggests (`choices`' index). */
  recommended?: number | null
  /** decision: `false` when only an option answers it (an agent's own question); missing: own words answer it too. */
  ownWords?: boolean
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
  /** agent: `files` holds the voice note of this message, which plays at the top of its bubble. */
  voiceNote?: boolean
  /** agent: an artifact the bot published (or updated) in this turn. */
  artifact?: ArtifactCard
  callSeconds?: number
  /** The voice call it was said in (user), answered during (agent), or that it ends (notice): the chat shows those
   * messages only in the call's conversation. */
  call?: string
  /** user: `voice` when a call's voice wrote it for the user (`send_to_bot`), `host` when the host did (what a
   * thread was told). */
  from?: string
  /** The thread this chat entry comes from (its approval card): its id, and the title of the task it works on. */
  thread?: { id: string; title: string }
  /** The task of the bot's box that thread works on: its id. */
  task?: string | null
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

const shownOf = new WeakMap<Entry[], Entry[]>()

/**
 * `list` as the views read it, without the messages taken back (`unqueue`): the store keeps those for its rev guard
 * against a late response. One array per list, because the chat's memos depend on its identity.
 */
export function withoutRemoved(list: Entry[]): Entry[] {
  let shown = shownOf.get(list)
  if (!shown) {
    shown = list.filter((e) => !e.data.removed)
    shownOf.set(list, shown)
  }
  return shown
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

/** The quote a sending reply shows until the computer's own replaces it; of `part`, the text selected in the message, when there is one. */
export function quoteOf(e: Entry, part = ''): ReplyQuote {
  const names = [...(e.data.attachments ?? []), ...(e.data.files ?? [])].map((f) => f.name).join(', ')
  const quote: ReplyQuote = { id: e.id, author: e.kind === 'user' ? 'user' : (e.data.author ?? e.botId), text: (e.data.text || names).slice(0, 500) }
  if (e.data.artifact) quote.artifact = e.data.artifact.id
  else if (part.trim()) Object.assign(quote, { text: part.trim().slice(0, 4000), part: true })
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

/** One login profile's plan windows: none while it has no reading yet. */
export interface UsageLoginProfile {
  id: string
  name: string
  windows: UsageWindow[]
  updatedAt: number
  /** Since when its checks fail, which leaves `windows` from before. Absent while they work. */
  failedAt?: number
}

export interface UsageProvider {
  id: string
  name: string
  /** The default login's windows. */
  windows: UsageWindow[]
  source: string
  updatedAt: number
  /** Since when the default login's checks fail, which leaves `windows` from before. Absent while they work. */
  failedAt?: number
  /** The agent's ACP registry id, which its logo goes by. Absent from a host that doesn't name it. */
  registry?: string
  /** What the default login is called, when the agent has login profiles to tell it from. */
  defaultLoginProfile?: string
  /** The agent's signed-in account login profiles, each with its own windows. Absent when it has one login. */
  loginProfiles?: UsageLoginProfile[]
}

/** An agent the Usage screen can show or hide. The host keeps the choice, so every app follows it. */
export interface UsageService {
  id: string
  name: string
  shown: boolean
  /** The agent's ACP registry id, which its logo goes by. */
  registry?: string
}

export interface Usage {
  providers: UsageProvider[]
  /** Absent from a host older than the chooser. */
  services?: UsageService[]
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
  /** Its login profiles beside the default one (absent from older hosts, which hide the feature). */
  loginProfiles?: LoginProfile[]
  /** The login the computer already has, beside `loginProfiles`: the name the user gave it and who it signs in, when known. */
  defaultLoginProfile?: { name: string | null; label?: string | null; email?: string | null }
  /** How an account profile of it signs in: its CLI in a terminal, or the agent itself in a browser on the computer. Null: it keeps one sign-in of its own, so it only takes API key profiles. */
  loginProfileSignIn?: 'terminal' | 'browser' | null
  /** The variable names its API key usually goes in, which the key form starts with. */
  loginKeys?: string[]
}

/** One more login to an agent, beside the one the computer already has: the agent's own sign-in in a folder of the profile's (`account`), or named environment variables (`apiKey`). */
export interface LoginProfile {
  id: string
  kind: 'account' | 'apiKey'
  /** The user's name for it, else who it signs in once it has. */
  name: string | null
  /** The name the user gave it. */
  label?: string | null
  email?: string | null
  signedIn: boolean
  /** An API key profile's variable names; their values never leave the host. */
  vars?: string[]
}

/** The login profiles a bot on `backend` can pick. One that isn't signed in can't run a bot; `picked`, the bot's own, stays listed so its row shows it. */
export const loginProfileChoices = (backend: Backend, picked: string | null): LoginProfile[] =>
  (backend.loginProfiles ?? []).filter((p) => p.signedIn || p.id === picked)

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
  iconsRevision?: number
  icons?: import('./custom-icons').CustomIcons
  hostId: string
  name: string
  version: string
  /** The host's store schema, which the mirror's rows were synced under (absent from older hosts). */
  schema?: number
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
  /** A bot with a new message moves to the top of the roster (`setAutoOrder`). Absent from older hosts. */
  autoOrder?: boolean
  /** Days after a thread last ended that the computer deletes it, 0 never (`setThreadKeepDays`); it lists older threads
   * (`olderThreads`). Absent from older hosts. */
  threadKeepDays?: number
  /** The user can start a thread with a bot themselves (`startThread`). Absent from older hosts. */
  startThread?: boolean
  /** A thread may start without a bot, on an agent the user names (`startThread` without `botId`). */
  threadsWithoutBot?: boolean
  /** A thread may be started to build one workflow (`startThread` with `setup.workflow`). */
  workflowThreads?: boolean
  /** A thread may be started on a login of its agent (`startThread` with `setup.loginProfile`). */
  threadLogins?: boolean
  /** A thread may start with files (`startThread` with `attachments`; `upload` without `botId` for one without a bot). */
  threadFiles?: boolean
  /** A routine may name the folder its runs work in (`saveRoutine` with `folder`). Absent from older hosts. */
  routineFolder?: boolean
  /** `resetCredentials` can be used on this computer (absent from older hosts, false on a Codync Cloud computer). */
  credentialsReset?: boolean
  /** The host can store workflows. Missing means it cannot. */
  workflows?: boolean
  /** This host honours `updateBot`'s `paused`. Missing on an older host, which ignores the field. */
  botPause?: boolean
  /** Bots have `projects` and `worktreeMode`, and `cwd` is always the bot's own folder. */
  projects?: boolean
  /** A new thread may name its place in a git project (`startThread` with `setup.place`). */
  threadPlace?: boolean
  /** Bots have `sharedFolders`, linked into each new worktree of their projects. */
  worktreeFill?: boolean
  /** A thread without a bot sent no `cwd` gets a folder of its own (an older host: the home folder). */
  threadFolders?: boolean
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

export interface RecentWorkspace { path: string; usedAt: number }

export interface DirListing {
  recent?: RecentWorkspace[]
  ancestors?: DirItem[]
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

/** A signed-in phone that scanned a computer in no account, waiting to be let in (loopback `pairingRequests`). */
export interface PairingRequest {
  requestId: string
  deviceName: string
  platform?: string | null
  /** The phone's account, which the computer can join. */
  email?: string | null
  /** The 6 digits the phone shows too. */
  code: string
  /** The phone is signed in and the Codync cloud is on, so the computer can join its account. */
  canJoin?: boolean | null
  /** The computer can stay out of an account: the phone then connects only over Wi-Fi or Tailscale. */
  phoneOnly?: boolean | null
  /** The phones that lose access if the computer joins. */
  removes?: string[] | null
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
  /** In no account while it could join one: a signed-in phone that pairs asks to bring it into its account. Absent from older hosts. */
  canJoinAccount?: boolean | null
  /** Turned on, the cloud would leave it in no account on a Codync cloud; known with the cloud off too. Absent from older hosts, which just turn on. */
  joinsAccountOnEnable?: boolean | null
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
  /** Null for the login the computer already has: sent as is, so the host clears it. */
  loginProfile?: string | null
  cwd: string
  permission: string
  /** Null for the agent's default: sent as is, so the host clears it. */
  mode?: string | null
  model?: string | null
  modelOptions?: Record<string, string>
  fastMode?: boolean
  fallbacks?: Fallback[]
  selfFallbacks?: boolean
  /** Left out for a host before Advisor mode; null turns it off. */
  threadAgent?: Fallback | null
  threadFallbacks?: Fallback[]
  pinned?: boolean
  /** A name puts the bot in that roster section, null takes it out; left out, the host keeps it. */
  section?: string | null
  hidden?: boolean
  /** Pause the bot: it answers nothing and spends nothing until it is resumed; left out, the host keeps it. */
  paused?: boolean
  notify?: boolean | null
  connectors?: string[]
  skills?: string[]
  computer?: boolean
  manageTeam?: boolean
  offeredToNetwork?: boolean
  reachableByAssistants?: boolean
  sharedMemory?: boolean
  harnessMemory?: boolean
  /** Left out where the bot has none: the host keeps what it has. */
  compaction?: CompactionThreshold
  /** The most threads at once, 1 to 6; left out for a host before the box. */
  maxThreads?: number
  /** Left out for a host before worktrees, and for one with `worktreeMode`. */
  threadWorktrees?: boolean
  /** The whole list, the default first; left out for a host before projects. */
  projects?: string[]
  /** Left out for a host before projects. */
  worktreeMode?: WorktreeMode
  /** The whole map, per project; left out for a host before shared folders. */
  sharedFolders?: Record<string, string[]>
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
    loginProfile: bot.loginProfile ?? null,
    cwd: bot.managedWorkspace ? '' : bot.cwd,
    permission: bot.permission,
    mode: bot.mode,
    model: bot.model ?? null,
    modelOptions: bot.modelOptions,
    fastMode: bot.fastMode,
    fallbacks: bot.fallbacks,
    selfFallbacks: bot.selfFallbacks ?? undefined,
    ...(bot.threadFallbacks === null ? {} : { threadAgent: bot.threadAgent, threadFallbacks: bot.threadFallbacks }),
    pinned: bot.pinned,
    hidden: bot.hidden,
    notify: bot.notify ?? null,
    connectors: bot.connectors,
    skills: bot.skills,
    computer: bot.computer,
    manageTeam: bot.manageTeam ?? undefined,
    offeredToNetwork: bot.offeredToNetwork ?? undefined,
    reachableByAssistants: bot.reachableByAssistants ?? undefined,
    compaction: bot.compaction ?? undefined,
    maxThreads: bot.maxThreads ?? undefined,
    ...(bot.worktreeMode === null
      ? { threadWorktrees: bot.threadWorktrees ?? undefined }
      : { projects: bot.projects, worktreeMode: bot.worktreeMode }),
    paused: bot.paused,
    ...(bot.sharedFolders === null ? {} : { sharedFolders: bot.sharedFolders }),
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
