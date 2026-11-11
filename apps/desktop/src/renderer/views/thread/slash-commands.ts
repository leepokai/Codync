// The composer's `/` commands, listed while the draft is just `/partial`: Fast mode runs here
// through the host's `updateBot` (a new session is in the bot's menu); a skill fills in `/<id> `, and the host turns that message into "read this skill first"
// (host/src/market/skills.rs `expand_command`). A group lists its members' commands: Fast mode
// for all of them, and every skill one of them has. docs/features/marketplace.md, "Commands".
import { fastModeDetail, fastModeShown, fastModeState, groupFastMode, type FastModeState } from '../../../shared/fast-mode.ts'
import type { Bot } from '../../../shared/models.ts'

/** The tools every bot can be asked to work with: `/n8n …` and `/comfy …`. The host adds a note and the bot uses the tool's own tools (host/src/chat/tool_commands.rs). */
const TOOL_COMMANDS = [
  { name: 'n8n', icon: 'point.3.connected.trianglepath.dotted', title: 'n8n', detail: "Do it with the computer's n8n workflows" },
  { name: 'comfy', icon: 'photo', title: 'ComfyUI', detail: 'Make an image with ComfyUI' },
]

export type SlashAction = { kind: 'fast' } | { kind: 'skill'; id: string }

export interface SlashCommand {
  /** What follows the `/`. */
  name: string
  icon: string
  title: string
  detail: string
  warning: boolean
  action: SlashAction
}

/** The command name being typed after a leading `/`, before any space; otherwise null. */
export function slashQuery(draft: string): string | null {
  return /^\/\S*$/.test(draft) ? draft.slice(1) : null
}

/** The bot's built-in commands; a group's Fast mode is its members' (`groupFastMode`). */
function builtIn(bot: Bot, members: Bot[]): SlashCommand[] {
  const fast = bot.kind === 'group' ? groupFastMode(members) : ownFastMode(bot)
  return fast ? [{ name: 'fast', icon: 'bolt', title: 'Fast mode', detail: fast.detail, warning: fast.state === 'refused', action: { kind: 'fast' } }] : []
}

/** A bot's own Fast mode, while it shows it. */
function ownFastMode(bot: Bot): { state: FastModeState; detail: string } | null {
  return fastModeShown(bot) ? { state: fastModeState(bot), detail: fastModeDetail(bot) } : null
}

/**
 * Built-in commands, then the bot's skills (ids in `bot.skills`; a group's `members` have them), then the tool
 * commands, whose name or title holds `query`.
 */
export function slashCommands(bot: Bot | null, query: string, installedSkills: { id: string; name: string; description: string }[], members: Bot[] = []): SlashCommand[] {
  if (!bot) return []
  const enabled = bot.kind === 'group' ? members.flatMap((m) => m.skills) : bot.skills
  const skills = installedSkills
    .filter((s) => enabled.includes(s.id))
    .map((s): SlashCommand => ({ name: s.id, icon: 'book.pages', title: s.name, detail: s.description, warning: false, action: { kind: 'skill', id: s.id } }))
  // A skill with the same id keeps the name: the host expands the skill first.
  const tools = TOOL_COMMANDS.filter((t) => !skills.some((s) => s.name === t.name)).map((t): SlashCommand => ({ ...t, warning: false, action: { kind: 'skill', id: t.name } }))
  const q = query.toLowerCase()
  return [...builtIn(bot, members), ...skills, ...tools].filter((c) => c.name.includes(q) || c.title.toLowerCase().includes(q))
}

/** A draft that is just a built-in command (`/fast`): sending it runs the command. */
export function builtInCommand(bot: Bot | null, draft: string, members: Bot[] = []): SlashCommand | null {
  const name = slashQuery(draft.trim())
  return bot && name !== null ? (builtIn(bot, members).find((c) => c.name === name) ?? null) : null
}

/**
 * What Return does with `picked` highlighted: run it, except a skill whose name is already
 * typed out, which sends.
 */
export function returnRuns(picked: SlashCommand, draft: string): boolean {
  return picked.action.kind !== 'skill' || draft !== `/${picked.name}`
}
