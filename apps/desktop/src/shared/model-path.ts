/**
 * A bot's agent, login and model as one string, the way the roster's hover tip and the Model
 * Manager row both name it: `Codex · gpt-5.3-codex`, or `OpenCode · Work ·
 * opendesign/deepseek-v4.1-flash-eco` for a bot that picks a login profile. Null when there is
 * nothing to name — a custom command has no model of its own, and a bot with no model set has no
 * path either.
 *
 * One implementation, so the roster and the manager never drift apart, and one separator, so the
 * same path reads the same in both (the MM sync rule). The parameters stay loose on purpose: the
 * manager builds its rows from a host snapshot it hasn't narrowed yet, where a backend can still be
 * a custom command's object rather than an id, and a model can still arrive as a number.
 */
export function modelPath(
  bot: { backend?: unknown; model?: unknown },
  agentName?: string | null,
  loginName?: string | null,
): string | null {
  if (typeof bot.backend !== 'string' || bot.backend === 'custom') return null
  if (typeof bot.model !== 'string' || !bot.model.trim()) return null
  const agent = agentName?.trim() || bot.backend
  const login = loginName?.trim()
  return [agent, ...(login ? [login] : []), bot.model.trim()].join(MODEL_PATH_JOIN)
}

/** What sits between the segments of a model path: an agent, an optional login, the model id. */
export const MODEL_PATH_JOIN = ' · '

/**
 * The agent's display name as the app names it, or null for one hello hasn't named — `modelPath`
 * then shows the raw id it runs under, so a path is never left blank. `backends` is a computer's
 * hello; a bot's backend can still be an object there, which names nothing.
 */
export function agentNameOf(backends: ReadonlyArray<{ id: string; name: string }>, backend: unknown): string | null {
  return typeof backend === 'string' ? backends.find((b) => b.id === backend)?.name ?? null : null
}

/**
 * The name of the login profile a bot runs on, when it picks one and the computer has named it. The
 * computer's own login has no id, so it adds nothing; a profile the computer hasn't named yet has
 * no name to show, so it adds nothing either — an unnamed profile's id is a uuid, which reads as
 * noise.
 */
export function loginNameOf(
  backends: ReadonlyArray<{ id: string; loginProfiles?: { id: string; name: string | null }[] | null }>,
  backend: unknown,
  loginProfile: unknown,
): string | null {
  if (typeof backend !== 'string' || typeof loginProfile !== 'string' || !loginProfile) return null
  const profile = backends.find((b) => b.id === backend)?.loginProfiles?.find((p) => p.id === loginProfile)
  return profile?.name?.trim() || null
}

/**
 * The roster row's hover tip: its heading (the bot's name, with the computer's when the roster
 * spans more than one), then the model under it. A line with nothing to say is left out, so a
 * bot without a model keeps a one-line tip rather than a blank second line.
 *
 * A line feed is a line break in a `title` attribute (HTML's `title`, Blink's tooltip), which is
 * the app's own tooltip — no custom bubble is invented for this.
 */
export function hoverTip(lines: (string | null | undefined)[]): string {
  return lines.filter((line): line is string => !!line && !!line.trim()).join('\n')
}
