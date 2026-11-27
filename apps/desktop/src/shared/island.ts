import type { BotStatus, Entry, UsageProvider } from './models'

// The notch island: a black shape at the top of the screen (in the notch on a
// MacBook) that shows working bots and takes approvals. The chat window owns the stores and
// sends an `IslandSummary`; the island window only draws it (main/island.ts, views/island/).

/** One tool call in a bot's ticker. */
export interface IslandStep {
  title: string
  /** The ACP tool kind: execute | edit | read | search | fetch | … */
  kind: string
}

/** A final agent reply in a bot's own chat. */
export interface IslandReply {
  id: string
  text: string
}

/** One bot as the island shows it (`ref` = computerId/botId). */
export interface IslandBot {
  customIcon?: import('./custom-icons').CustomIcon
  ref: string
  name: string
  shape: string
  /** Its avatar palette id. */
  color: string
  /** How it stands outside its chat (`outsideStatus`): what its face, badge and dot show, and whether the island
   * counts it. A `BotStatus`, or a newer one this app doesn't know. */
  status: BotStatus | string
  /** Its chat's own status: done and failed follow the chat's turns. */
  chat: BotStatus | string
  /** True while the bot's running turn talks in another conversation (e.g. a group room). */
  away?: boolean
  /** The reply from a live turn completion, or null when there is none. */
  completion?: IslandReply | null
  /** The newest final reply on a host that doesn't report completions. */
  reply?: IslandReply | null
  legacyStreamRev?: number
  /** The agent it runs, as the app names it ("Claude Code"). */
  backend: string
  /** Its working folder's name. */
  folder: string
  /** What it's doing, that it needs you, or its last message. */
  detail: string
  /** Its latest tool calls, oldest first (only while it works). */
  steps: IslandStep[]
  /** Why it stopped, when it failed. */
  error: string | null
  /** The newest error notice entry id in this bot's own chat, or null if none. */
  errorId?: string | null
}

export interface IslandOption {
  id: string
  label: string
  /** allow_once | allow_always | reject_once | reject_always, or another the agent sent. */
  kind: string
}

/** A pending approval card: the conversation holding it and the bot asking. */
export interface IslandPermission {
  ref: string
  entryId: string
  bot: Pick<IslandBot, 'name' | 'shape' | 'color' | 'backend' | 'customIcon'>
  headline: string
  title: string
  options: IslandOption[]
  /** The option on its way to the host. */
  answering: string | null
}

export interface IslandSummary {
  bots: IslandBot[]
  permissions: IslandPermission[]
  /** This computer's plan limits by agent (Claude, Codex); the island shows each agent's login nearest a limit. */
  usage: UsageProvider[]
}

/** The display the island sits on: the notch's size in points, or null at a plain top edge. */
export interface IslandScreen {
  notch: { width: number; height: number } | null
}

/** The island's shape in its window, in points: the part that takes the mouse. */
export interface IslandRect {
  x: number
  y: number
  width: number
  height: number
}

/** One `NSScreen`: its frame's x and width, the safe area's top inset and the menu bar on each side of the notch. */
export interface NotchScreen {
  x: number
  width: number
  top: number
  left: number
  right: number
}

/** Closed shapes, then the open views (640 pt wide, under a header of tabs). */
export type IslandMode = 'hidden' | 'peek' | 'compact' | 'overview' | 'empty' | 'approval' | 'finished' | 'failed' | 'message' | 'usage'
/** The header's choice: `auto` shows what needs you first (an approval, a finished or failed bot). */
export type IslandTab = 'auto' | 'bots' | 'message' | 'usage'
/** A bot that just stopped, shown until it times out (done) or is dismissed (failed). */
export interface IslandNotice {
  kind: 'finished' | 'failed'
  ref: string
  /** The fresh reply or failure text from the turn that just ended; never a stale chat preview. */
  text?: string | null
}

/** Per-bot turn baseline tracked across summaries so only fresh own-chat turns trigger a notice. */
export interface BotTurnState {
  chat: string
  away: boolean
  completionId?: string
  legacyStreamRev?: number
  baselineReplyId: string | null
  baselineErrorId: string | null
}

/** How long a bot that just finished stays in the island. */
export const FINISHED_MS = 5200
/** Extra room around the shape that still counts as on it, so a click at its edge is never lost. */
export const HIT_MARGIN = 8
/** The window holding the island: the largest shape fits, the rest is transparent and lets clicks through. */
export const PANEL = { width: 720, height: 300 }
/** The open island's width. */
export const OPEN_WIDTH = 640

/**
 * Every open view has the same height (for a 32 pt notch; a taller one adds its extra), so
 * switching tabs or alerts never moves the black shape: only what's inside changes.
 */
const OPEN_HEIGHT = 196
/** The resting bar at a plain top edge (no notch). */
const BAR = 26

/** Working or waiting on the user. */
export const inTurn = (status: string) => status === 'working' || status === 'needsInput'
/** The bot is at work, in its chat or in the background: what the island shows and counts, as the tray does. */
export const isActive = (b: Pick<IslandBot, 'status'>) => inTurn(b.status)
export const isOpenMode = (mode: IslandMode) => mode !== 'hidden' && mode !== 'peek' && mode !== 'compact'
/** Leaving the message tab hides its recording pill, so dictation still going there stops (keeping the words). */
export const stopsDictation = (from: IslandTab, to: IslandTab, dictating: boolean) => dictating && from === 'message' && to !== 'message'

export function islandMode(
  summary: IslandSummary,
  { open, peek, tab, deferred, notice }: { open: boolean; peek: boolean; tab: IslandTab; deferred: ReadonlySet<string>; notice: IslandNotice | null },
): IslandMode {
  if (!open) {
    // Alerts open the island by themselves.
    if (summary.permissions.some((p) => !deferred.has(p.entryId))) return 'approval'
    if (notice) return notice.kind
    if (peek) return 'peek'
    return summary.bots.some(isActive) ? 'compact' : 'hidden'
  }
  if (tab === 'message' || tab === 'usage') return tab
  if (tab === 'auto') {
    if (summary.permissions.length) return 'approval'
    if (notice) return notice.kind
  }
  return summary.bots.some(isActive) ? 'overview' : 'empty'
}

/** The indicated final reply, or the newest one on hosts without a completion id. */
export function islandReply(
  entries: readonly Entry[],
  formatPreview: (text: string) => string = (t) => t,
  replyId?: string,
): IslandReply | null {
  const entry = entries.findLast((e) =>
    e.kind === 'agent' && e.data.final === true && !e.data.call && !e.data.removed && (replyId === undefined || e.id === replyId),
  )
  if (!entry) return null
  const raw =
    entry.data.text?.trim() ||
    entry.data.files?.map((f) => f.name).filter(Boolean).join(', ') ||
    entry.data.attachments?.map((a) => a.name).filter(Boolean).join(', ') ||
    entry.data.artifact?.title?.trim() ||
    ''
  return { id: entry.id, text: raw ? formatPreview(raw) : '' }
}

/** The newest error notice in a bot's own chat. */
export function islandLastError(entries: readonly Entry[]): { id: string; text: string } | null {
  const entry = entries.findLast((e) => e.kind === 'notice' && e.data.style === 'error' && !e.data.removed)
  return entry ? { id: entry.id, text: entry.data.text ?? '' } : null
}

/** Tool calls from the bot's active turn and conversation, never stale calls from an earlier chat turn. */
export function islandTurnSteps(
  bot: { id: string; status: string; workingChat?: string | null; startedAt?: number | null },
  ownEntries: readonly Entry[],
  workingEntries: readonly Entry[],
  limit: number,
  fallbackTitle: string,
): IslandStep[] {
  if (!inTurn(bot.status)) return []
  const away = Boolean(bot.workingChat && bot.workingChat !== bot.id)
  const source = away ? workingEntries.filter((e) => e.data.author === bot.id) : ownEntries
  const since = bot.startedAt
  return source
    .filter((e) => e.kind === 'tool' && !e.data.removed && (since == null || e.createdAt >= since))
    .slice(-limit)
    .map((e) => ({ title: e.data.title || fallbackTitle, kind: e.data.toolKind ?? 'other' }))
}

/** Advances the per-bot turn baselines, preserving pre-turn ids while a turn is running. */
export function nextTurnStates(before: ReadonlyMap<string, BotTurnState> | null, bots: IslandBot[]): Map<string, BotTurnState> {
  const next = new Map<string, BotTurnState>()
  for (const b of bots) {
    const prev = before?.get(b.ref)
    const active = inTurn(b.chat)
    next.set(b.ref, {
      chat: b.chat,
      away: active && Boolean(b.away),
      completionId: b.completion?.id ?? prev?.completionId,
      legacyStreamRev: b.legacyStreamRev,
      baselineReplyId: active && prev && prev.legacyStreamRev === b.legacyStreamRev ? prev.baselineReplyId : (b.reply?.id ?? null),
      baselineErrorId: active && prev ? prev.baselineErrorId : (b.errorId ?? null),
    })
  }
  return next
}

/** Bots whose own-chat turn just ended with a new reply or failure; skips bot-to-bot, group, and silent turns. */
export function transitions(before: ReadonlyMap<string, BotTurnState>, bots: IslandBot[]): IslandNotice[] {
  return bots.flatMap((b): IslandNotice[] => {
    const prev = before.get(b.ref)
    const completion = b.completion?.id !== prev?.completionId ? b.completion : null
    const ownTurn = prev && inTurn(prev.chat) && !prev.away
    if (b.chat === 'error') {
      const freshError = b.errorId != null && b.errorId !== prev?.baselineErrorId
      if (ownTurn || (completion && freshError)) return [{ kind: 'failed', ref: b.ref, text: freshError ? b.error : null }]
    }
    if (completion) return [{ kind: 'finished', ref: b.ref, text: completion.text || null }]
    if (!ownTurn || b.chat !== 'idle') return []
    if (b.completion !== undefined || b.legacyStreamRev !== prev.legacyStreamRev || !b.reply || b.reply.id === prev.baselineReplyId) return []
    return [{ kind: 'finished', ref: b.ref, text: b.reply.text || null }]
  })
}

/** The bot in the big slot: one that needs you, else one working, else the first. */
export function focusOf(bots: IslandBot[]) {
  return bots.find((b) => b.status === 'needsInput') ?? bots.find(isActive) ?? bots[0] ?? null
}

/** The other bots, working and waiting first, at most `limit`. */
export function othersOf(bots: IslandBot[], focus: IslandBot | null, limit: number) {
  const rest = bots.filter((b) => b.ref !== focus?.ref)
  return [...rest.filter(isActive), ...rest.filter((b) => !isActive(b))].slice(0, limit)
}

/** The header's height: a taller notch pushes the content down so nothing sits behind the camera. */
export const contentTop = (screen: IslandScreen) => 36 + Math.max(0, (screen.notch?.height ?? 32) - 32)

/** The shape's size and bottom corner radius for `mode`. */
export function islandFrame(mode: IslandMode, screen: IslandScreen) {
  const notch = screen.notch
  const bar = notch?.height ?? BAR
  const closedRadius = notch ? 14 : bar / 2
  switch (mode) {
    case 'hidden':
      // At a plain top edge only a thin strip under the edge wakes it.
      return notch ? { width: notch.width, height: notch.height, radius: closedRadius } : { width: 160, height: 0, radius: 0 }
    case 'peek':
      return { width: notch ? notch.width + 64 : 120, height: bar, radius: closedRadius }
    case 'compact':
      return { width: notch ? notch.width + 104 : 240, height: bar, radius: closedRadius }
    default:
      return { width: OPEN_WIDTH, height: OPEN_HEIGHT + contentTop(screen) - 36, radius: 30 }
  }
}

/** The shape centered at the top of a `windowWidth`-wide window. */
export function islandRect(frame: { width: number; height: number }, windowWidth: number): IslandRect {
  return { x: (windowWidth - frame.width) / 2, y: 0, width: frame.width, height: frame.height }
}

/** The part of the window that takes the mouse: the shape with `HIT_MARGIN` around it, inside the window. */
export function hitRect(rect: IslandRect): IslandRect {
  if (rect.width <= 0) return { x: 0, y: 0, width: 0, height: 0 }
  const x = Math.max(0, Math.floor(rect.x - HIT_MARGIN))
  const y = Math.max(0, Math.floor(rect.y - HIT_MARGIN))
  return { x, y, width: Math.ceil(rect.x + rect.width + HIT_MARGIN) - x, height: Math.ceil(rect.y + rect.height + HIT_MARGIN) - y }
}

/** Whether a point in window coordinates is on the island. The window's top edge is the screen's, so above it counts too. */
export function hits(rect: IslandRect, x: number, y: number) {
  const r = hitRect(rect)
  return r.width > 0 && x >= r.x && x <= r.x + r.width && y <= r.y + r.height && (r.y === 0 || y >= r.y)
}

/** The notch of the display at `bounds`, from the screens `NSScreen` reports (same points and x axis). */
export function notchOf(screens: NotchScreen[], bounds: { x: number; width: number }) {
  const s = screens.find((s) => s.x === bounds.x && s.width === bounds.width && s.top > 0)
  const width = s ? s.width - s.left - s.right : 0
  return s && width > 0 ? { width, height: s.top } : null
}

/** The setting's name: a notch on the Mac, the top edge elsewhere. */
export const islandLabel = (platform: string) => (platform === 'darwin' ? 'Show in Notch' : 'Show at top of screen')
