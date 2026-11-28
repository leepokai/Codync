/**
 * A bot's agent, login and model as one string, the way the roster's hover tip and the Model
 * Manager's row both name it: `Codex->gpt-5.3-codex`, or
 * `OpenCode->Work->opendesign/deepseek-v4.1-flash-eco` for a bot that picks a login profile. Null
 * when there is nothing to name — a custom command has no model of its own, and a bot with no model
 * set has no path either.
 *
 * The manager's row builds the same shape from its own `rowModelPath` on its branch. This is the
 * roster's builder for that string; the manager's row cases are pinned in the test so the roster
 * prints what the manager documents. The parameters stay loose on purpose: a row can arrive from a
 * host snapshot that hasn't been narrowed yet, where a backend can still be a custom command's
 * object rather than an id, and a model can still arrive as a number.
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
  return [agent, ...(login ? [login] : []), bot.model.trim()].join('->')
}

/**
 * The name of the login profile a bot runs on, when it picks one and the computer has named it. The
 * computer's own login has no id, so it adds nothing; a profile the computer hasn't named yet has no
 * name to show either — an unnamed profile's id is a uuid, which would read as noise.
 *
 * This is the same lookup the Model Manager's row does, kept here so the roster names a login the
 * way the manager does.
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
