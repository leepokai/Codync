/**
 * A bot's agent and model as one string, the way the roster's hover tip and the Model Manager
 * both name it: `Codex->gpt-5.3-codex`. Null when there is nothing to name — a custom command
 * has no model of its own, and a bot with no model set has no path either.
 *
 * One implementation, so the roster and the manager never drift apart. The parameters stay loose
 * on purpose: the manager builds its rows from a host snapshot it hasn't narrowed yet, where a
 * backend can still be a custom command's object rather than an id.
 */
export function modelPath(bot: { backend?: unknown; model?: unknown }, agentName?: string | null): string | null {
  if (typeof bot.backend !== 'string' || bot.backend === 'custom') return null
  if (typeof bot.model !== 'string' || !bot.model.trim()) return null
  const agent = agentName?.trim() || bot.backend
  return `${agent}->${bot.model.trim()}`
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
