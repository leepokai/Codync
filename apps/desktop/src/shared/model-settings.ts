/** A model's own setting (effort, fast…) and the values the agent accepts. */
export interface ModelSetting {
  id: string
  name: string
  values: { id: string; name: string }[]
  default?: string | null
  /** The ACP category, when the agent gives one: `thought_level` is the effort level. */
  category?: string | null
}

interface Catalog {
  models: { id: string; options?: ModelSetting[] }[]
  currentModelId?: string | null
}

/** The settings to show for a bot's model. Default (`''`) shows those of the model the agent
 *  runs by default, which is the one a bot on Default gets. */
export function modelSettings(catalog: Catalog | null | undefined, model: string): ModelSetting[] {
  const id = model || catalog?.currentModelId
  return catalog?.models.find((m) => m.id === id)?.options ?? []
}


/** The agent's effort level: the setting of ACP category `thought_level`, whatever its id. */
export function effortSetting(settings: ModelSetting[]): ModelSetting | undefined {
  return settings.find((s) => s.category === 'thought_level')
}

/** What the bot's running agent session says, as the bot payload carries it. */
export interface SessionRuntime {
  agentStep: { step: number } | null
  sessionLive?: boolean
  sessionOptions?: Record<string, string> | null
  sessionSettings?: ModelSetting[] | null
  settingIssues?: Record<string, { issue: string; value: string }>
  restartPending?: boolean
}

/** What one step of the bot's chain (0: its own agent, n: its nth fallback) runs with, when that step's agent is the one
 *  running; `null` otherwise, since another agent's session says nothing about it. */
export interface SessionReport {
  live: boolean
  /** Setting id → value, as the session last listed them; null: none listed. */
  options: Record<string, string> | null
  settings: ModelSetting[] | null
  issues: Record<string, { issue: string; value: string }>
  /** A change waits for the running reply, then starts a new agent session. */
  pending: boolean
}

export function reportFor(bot: SessionRuntime, step: number): SessionReport | null {
  if ((bot.agentStep?.step ?? 0) !== step) return null
  return {
    live: bot.sessionLive ?? false,
    options: bot.sessionOptions ?? null,
    settings: bot.sessionSettings ?? null,
    issues: bot.settingIssues ?? {},
    pending: bot.restartPending ?? false,
  }
}

/** The Effort level row of one step: `none` (no row), `later` (unknown until the agent runs this model) or `row`, with
 *  the setting to pick from and one caption line (first match wins; nothing is inferred from a saved value). */
export function effortRow({ catalogSettings, saved, report, loading, failed, chosenModel }: {
  /** What the agent's catalog lists for this step's model. */
  catalogSettings: ModelSetting[]
  saved: string | undefined
  report: SessionReport | null
  loading: boolean
  failed: boolean
  /** The step names a model of its own (not Default). */
  chosenModel: boolean
}): { kind: 'none' | 'later' | 'row'; setting?: ModelSetting; caption: string | null } {
  // While nothing is pending, the running session's own list is for the model it runs.
  const session = report && !report.pending && report.settings ? effortSetting(report.settings) : undefined
  const setting = session ?? effortSetting(catalogSettings)
  const name = (v: string) => setting?.values.find((x) => x.id === v)?.name ?? v
  const row = (caption: string | null) => ({ kind: 'row' as const, setting, caption })
  if (!setting) {
    if (loading) return saved ? row(`Saved: ${saved} · checking this agent’s options`) : { kind: 'none', caption: null }
    if (failed) return saved ? row(`Saved: ${saved} · couldn’t check this agent’s options`) : { kind: 'none', caption: null }
    if (chosenModel && catalogSettings.length === 0 && !session) return { kind: 'later', caption: null }
    return saved ? row(`Saved: ${saved} · not offered for this model`) : { kind: 'none', caption: null }
  }
  const id = setting.id
  const reported = report?.options?.[id]
  const issue = report?.issues[id]
  if (issue && saved === issue.value) {
    if (issue.issue === 'rejected') return row(`Saved: ${name(saved)} · not applied (the agent refused it)`)
    return row(reported ? `In use: ${name(reported)} · Saved: ${name(saved)} not offered` : `Saved: ${name(saved)} · not offered for this model`)
  }
  if (report?.pending && saved !== reported) {
    const next = saved ? name(saved) : 'Default'
    return row(reported ? `In use: ${name(reported)} · ${next} pending` : `Current: unknown · ${next} pending`)
  }
  if (!report?.live) return row(saved ? `Not running · Saved: ${name(saved)}` : 'Not running')
  if (!reported) return row('Current: unknown')
  if (!saved) return row(`In use: ${name(reported)} (agent default)`)
  if (saved === reported) return row(`In use: ${name(reported)}`)
  return row(`Saved: ${name(saved)} · not confirmed (in use: ${name(reported)})`)
}

/** The line under a fallback's row, without starting its agent: its effort level as its own running session reports it,
 *  or else what is saved for it (Edit loads the agent's list to change it). */
export function fallbackLine(entry: { modelOptions: Record<string, string> }, report: SessionReport | null): string | null {
  const setting = report?.settings ? effortSetting(report.settings) : undefined
  if (setting) {
    const { caption } = effortRow({ catalogSettings: [setting], saved: entry.modelOptions[setting.id], report, loading: false, failed: false, chosenModel: false })
    if (caption) return `Effort level · ${caption}`
  }
  const saved = Object.values(entry.modelOptions)
  return saved.length ? `Saved: ${saved.join(', ')}` : null
}
