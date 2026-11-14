import { AUTO_MODE } from './approvals.ts'
import { isGroup, type Backend, type Bot, type BotDraft, type Fallback, type Resting } from './models.ts'

/** The host refuses more. */
export const MAX_FALLBACKS = 3

/** A login of the chain: an agent on one of its accounts, or (no account) on the login the computer already has. */
export type Login = { backend: string; account?: string | null }

/** Two steps are the same when they run the same agent on the same login. */
export const sameLogin = (a: Login, b: Login) => a.backend === b.backend && (a.account ?? null) === (b.account ?? null)

/** `login`'s rest, while it lasts. */
export const restOf = (bot: Bot, login: Login, now: number): Resting | null =>
  bot.resting.find((r) => sameLogin({ backend: r.agent, account: r.account }, login) && r.until > now) ?? null

/** The step the bot runs on right now, as a login; null on its own agent. */
export const stepLogin = (bot: Bot): Login | null => bot.agentStep && { backend: bot.agentStep.agent, account: bot.agentStep.account }

/** The logins of agent `backend` a new step can still take: its accounts that are signed in and, first as `null`, the computer's own, without the ones the chain has. */
export function freeLogins(backend: Backend, taken: Login[]): (string | null)[] {
  const own: (string | null)[] = [null]
  const all = own.concat((backend.accounts ?? []).filter((a) => a.signedIn).map((a) => a.id))
  return all.filter((account) => !taken.some((t) => sameLogin(t, { backend: backend.id, account })))
}

/** The agent a bot runs on right now: its fallback's while it's on one. */
export const runningAgent = (bot: Bot) => bot.agentStep?.agent ?? bot.backend

/** The bots run more than one agent, so the roster marks each with its agent's logo. */
export const mixesAgents = (bots: Bot[]) => new Set(bots.filter((b) => !isGroup(b)).map(runningAgent)).size > 1

/** "3:00 PM" today, "Tue 3:00 PM" another day. */
export function clockTime(ms: number, now: number): string {
  const at = new Date(ms)
  const time = at.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
  return at.toDateString() === new Date(now).toDateString() ? time : `${at.toLocaleDateString(undefined, { weekday: 'short' })} ${time}`
}

/** A resting agent's second line: "Usage resets 3:00 PM", or "Resting until 3:00 PM". */
export const restLine = (rest: Resting, now: number) =>
  `${rest.reason === 'usageLimit' ? 'Usage resets' : 'Resting until'} ${clockTime(rest.until, now)}`

/**
 * The Agent card's first line while the bot runs on a fallback; null on its own agent. `name` says what to call a login
 * ("Claude Code (Work)" on an account); `own` asks for the computer's own login by its account too, which tells two
 * logins of one agent apart.
 */
export function stepBanner(bot: Bot, name: (login: Login, own: boolean) => string, now: number): string | null {
  const step = stepLogin(bot)
  if (!step) return null
  const same = step.backend === bot.backend
  const [to, own] = [name(step, same), name(bot, same)]
  const rest = restOf(bot, bot, now)
  return rest ? `Using ${to} until ${own} resets at ${clockTime(rest.until, now)}` : `Using ${to} while ${own} can't answer`
}

/** The list with the fallback at `index` moved `by` places (clamped to the ends). */
export function moveFallback(list: Fallback[], index: number, by: number): Fallback[] {
  const next = [...list]
  const [moved] = next.splice(index, 1)
  if (moved) next.splice(Math.max(0, Math.min(next.length, index + by)), 0, moved)
  return next
}

/** The bot on agent `backend`, on the computer's own login: a fallback on that login goes (a login is in the chain once, as the host keeps it). */
export const withBackend = (d: BotDraft, backend: string): BotDraft => d.backend === backend ? d : ({
  ...d, backend, account: null, model: null, modelOptions: {}, fallbacks: d.fallbacks?.filter((f) => !sameLogin(f, { backend })),
  // A picked mode becomes the new agent's Auto mode (or, without one, asking).
  mode: d.mode ? AUTO_MODE : d.mode,
})
