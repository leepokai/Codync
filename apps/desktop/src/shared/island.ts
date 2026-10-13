import type { BotStatus, UsageProvider } from './models'

// The notch island: a black shape at the top of the screen (in the notch on a
// MacBook) that shows working bots and takes approvals. The chat window owns the stores and
// sends an `IslandSummary`; the island window only draws it (main/island.ts, views/island/).

/** One tool call in a bot's ticker. */
export interface IslandStep {
  title: string
  /** The ACP tool kind: execute | edit | read | search | fetch | … */
  kind: string
}

/** One bot as the island shows it (`ref` = computerId/botId). */
export interface IslandBot {
  ref: string
  name: string
  shape: string
  /** Its avatar palette id. */
  color: string
  /** A `BotStatus`, or a newer one this app doesn't know. */
  status: BotStatus | string
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
  bot: Pick<IslandBot, 'name' | 'shape' | 'color' | 'backend'>
  headline: string
  title: string
  options: IslandOption[]
  /** The option on its way to the host. */
  answering: string | null
}

export interface IslandSummary {
  bots: IslandBot[]
  permissions: IslandPermission[]
  /** This computer's plan limits (Claude, Codex), as the menu bar shows them. */
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

export const isActive = (b: Pick<IslandBot, 'status'>) => b.status === 'working' || b.status === 'needsInput'
export const isOpenMode = (mode: IslandMode) => mode !== 'hidden' && mode !== 'peek' && mode !== 'compact'

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

/** Bots that just stopped working since `before` (their last statuses by ref): done or failed. */
export function transitions(before: ReadonlyMap<string, string>, bots: IslandBot[]): IslandNotice[] {
  return bots.flatMap((b): IslandNotice[] => {
    const was = before.get(b.ref)
    if (was !== 'working' && was !== 'needsInput') return []
    if (b.status === 'idle') return [{ kind: 'finished', ref: b.ref }]
    if (b.status === 'error') return [{ kind: 'failed', ref: b.ref }]
    return []
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
export const islandLabel = (platform: string) => (platform === 'darwin' ? 'Show in notch' : 'Show at top of screen')
