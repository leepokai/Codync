import { AUTO_MODE } from './approvals.ts'
import { isGroup, type Backend, type Bot, type BotDraft, type Fallback, type FallbackReason, type Resting } from './models.ts'

/** The host refuses more. */
export const MAX_FALLBACKS = 3

/** A login of the chain: an agent on one of its login profiles, or (none) on the login the computer already has. */
export type Login = { backend: string; loginProfile?: string | null }

/** Two steps are the same when they run the same agent on the same login. */
export const sameLogin = (a: Login, b: Login) => a.backend === b.backend && (a.loginProfile ?? null) === (b.loginProfile ?? null)

/** `login`'s rest, while it lasts. */
export const restOf = (bot: Bot, login: Login, now: number): Resting | null =>
  bot.resting.find((r) => sameLogin({ backend: r.agent, loginProfile: r.loginProfile }, login) && r.until > now) ?? null

/** The step the bot runs on right now, as a login; null on its own agent. */
export const stepLogin = (bot: Bot): Login | null => bot.agentStep && { backend: bot.agentStep.agent, loginProfile: bot.agentStep.loginProfile }

/** The logins of agent `backend` a new step can still take: its login profiles that are signed in and, first as `null`, the computer's own, without the ones the chain has. */
export function freeLogins(backend: Backend, taken: Login[]): (string | null)[] {
  const own: (string | null)[] = [null]
  const all = own.concat((backend.loginProfiles ?? []).filter((p) => p.signedIn).map((p) => p.id))
  return all.filter((loginProfile) => !taken.some((t) => sameLogin(t, { backend: backend.id, loginProfile })))
}

/** The agents a step can use: each with a login the rest of the chain (`taken`) doesn't have, and the ones `keep` is on
 * while no other step has that login (a step keeps its own login, even one that has signed out since). */
export const stepAgents = (backends: Backend[], taken: Login[], keep: (Login | null)[] = []): Backend[] =>
  backends.filter((b) => keep.some((k) => k?.backend === b.id && !taken.some((t) => sameLogin(t, k))) || freeLogins(b, taken).length > 0)

/** The logins `step` can pick on its agent: the free ones, and its own, even one that has signed out since. */
export function stepLogins(agent: Backend, step: Login, taken: Login[]): (string | null)[] {
  const free = freeLogins(agent, taken)
  const own = step.loginProfile ?? null
  return free.includes(own) ? free : [...free, own]
}

/** A step on `agent`: `own` (the step before it changed agent) when it's on that agent and its login isn't elsewhere in the
 * chain (`taken`), else the agent's first free login with its default model. */
export const stepOn = (agent: Backend, taken: Login[], own: Fallback | null = null): Fallback =>
  own?.backend === agent.id && !taken.some((t) => sameLogin(t, own))
    ? own
    : { backend: agent.id, loginProfile: freeLogins(agent, taken)[0] ?? null, model: null, modelOptions: {} }

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

/** Why an agent rests, in the banner's words: "is signed out", "is rate limited"… A usage limit says when it resets instead, and a missing model or mode never rests an agent (`rest::record`). */
export function restPhrase(reason: FallbackReason): string {
  switch (reason) {
    case 'outOfCredits': return 'is out of credits'
    case 'rateLimited': return 'is rate limited'
    case 'overloaded': return 'is overloaded'
    case 'signedOut': return 'is signed out'
    case 'notStarting': return "couldn't start"
    default: return "can't answer"
  }
}

/** A sentence before it's translated: the phrase, and the names to put in once it's read in the app's language. */
export interface FallbackPhrase {
  key: string
  values: Record<string, string>
}

/**
 * The Agent card's first line while the bot runs on a fallback, as a phrase and its names; null on its own agent. `name`
 * says what to call a login ("Claude Code (Work)" on a login profile); `own` asks for the computer's own login by its
 * name too, which tells two logins of one agent apart. Translating the phrase before the names go in keeps a name that
 * holds " while " from being read as the sentence's own word.
 */
export function stepBannerPhrase(bot: Bot, name: (login: Login, own: boolean) => string, now: number): FallbackPhrase | null {
  const step = stepLogin(bot)
  if (!step) return null
  const same = step.backend === bot.backend
  const [to, own] = [name(step, same), name(bot, same)]
  const rest = restOf(bot, bot, now)
  if (!rest) return { key: "Using {to} while {own} can't answer", values: { to, own } }
  // A reset time is real only for a usage limit; another rest names what it is (signed out, rate limited…).
  return rest.reason === 'usageLimit'
    ? { key: 'Using {to} until {own} resets at {time}', values: { to, own, time: clockTime(rest.until, now) } }
    : { key: `Using {to} while {own} ${restPhrase(rest.reason)}`, values: { to, own } }
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
  ...d, backend, loginProfile: null, model: null, modelOptions: {}, fallbacks: d.fallbacks?.filter((f) => !sameLogin(f, { backend })),
  // A picked mode becomes the new agent's Auto mode (or, without one, asking).
  mode: d.mode ? AUTO_MODE : d.mode,
})
