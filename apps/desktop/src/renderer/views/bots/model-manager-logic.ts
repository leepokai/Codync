import type { RosterItem } from '../../store/app-model'
import { refKey } from '../../store/bot-ref.ts'
import { trNow } from '../../lib/l10n.ts'
import { agentModelsRequest } from './agent-models-query.ts'
import type { AgentModels } from '@shared/agent-models'

/** One agent, one model and its options, applied to every bot the user checked. */
export interface ApplyPlan {
  items: RosterItem[]
  backend: string
  model: string | null
  options: Record<string, string>
}

/** The checked bots, in roster order. Bot ids repeat across computers, so keys are `computer/bot`. */
export const filterChecked = (items: RosterItem[], checked: Set<string>) =>
  items.filter((i) => checked.has(refKey(i.ref)))

/** Why a row was not written, as the event counts it: the reason's text stays in the row, never in analytics. */
export type PlanRefusal = 'modelMissing' | 'catalogUnreadable' | 'settingsRejected'

/** One bot's fate: `ok` rows are saved, the rest carry the reason they were skipped and which count they add. */
export type PlanRow =
  | { item: RosterItem; ok: true }
  | { item: RosterItem; ok: false; reason: string; refusal: PlanRefusal }

/** `Not available on {computer}`, with the computer's name filled in. */
export const notAvailable = (computer: string) => trNow('Not available on {computer}').replace('{computer}', () => computer)

/** `Couldn't check {computer}'s models`: the catalog read failed, so the row is not written. */
export const notCheckable = (computer: string) => trNow("Couldn't check {computer}'s models").replace('{computer}', () => computer)

/** `Those settings aren't available on {computer}`: the model is offered, its values are not. */
export const settingsUnavailable = (computer: string) =>
  trNow("Those settings aren't available on {computer}").replace('{computer}', () => computer)

/**
 * Apply must read a catalog when a named model is chosen, or when settings travel with the agent's
 * default. A custom command has no catalog. An empty default does not need one.
 */
export const needsCatalog = (backend: string, model: string | null, options: Record<string, string>): boolean =>
  backend !== 'custom' && ((model?.trim() ?? '') !== '' || Object.keys(options).length > 0)

/** What a computer's catalog check found: its catalog, a read failure, or nothing to check against. */
export type CatalogCheck = { kind: 'ok'; catalog: AgentModels } | { kind: 'error' } | { kind: 'none' }

/**
 * The catalog call an Apply makes before it writes: always a new read (`refresh`), never the list
 * the computer kept. A kept list may be up to ten minutes old (the host answers a panel from it at
 * once), so judging a save by it can refuse a model the agent now offers or pass one it dropped.
 * Should the new read fail, the host keeps the old list for panels but this ask fails, and the row
 * is refused as unverifiable (`planRows`) instead of being written against a stale one.
 */
export const catalogRequest = (backend: string, loginProfile: string | null) =>
  agentModelsRequest(backend, loginProfile, true)

/**
 * The login a row keeps when `backend` is applied: its own when the agent stays the same, or the
 * default login when the bot moves to another agent (a profile belongs to one agent only).
 */
export const loginProfileFor = (bot: { backend?: unknown; loginProfile?: string | null }, backend: string): string | null =>
  bot.backend === backend && typeof bot.loginProfile === 'string' && bot.loginProfile !== '' ? bot.loginProfile : null

/** The minimal patch, plus the login snapshot the host must still have when it saves under its write lock. */
export const applyPatch = (item: RosterItem, plan: ApplyPlan): Record<string, unknown> => ({
  id: item.ref.botId,
  backend: plan.backend,
  loginProfile: loginProfileFor(item.bot, plan.backend),
  expectedLoginProfile: item.bot.loginProfile ?? null,
  model: plan.model,
  modelOptions: { ...plan.options },
  ...(plan.backend !== 'custom' ? { command: null } : {}),
})

/** Whether `options` are values `entry` lists. Empty settings need no list. */
const optionsFit = (entry: AgentModels['models'][number], options: [string, string][]): boolean => {
  if (options.length === 0) return true
  const listed = entry.options ?? []
  return options.every(([id, value]) => {
    const option = listed.find((candidate) => candidate.id === id)
    return option?.values.some((candidate) => candidate.id === value) ?? false
  })
}

/**
 * Which bots the plan can save to, decided before anything is written.
 *
 * With a model chosen, the catalog for each bot's own login must have verified it: a bot whose
 * agent-on-that-login doesn't offer the model is skipped with a reason, and so is one whose catalog
 * could not be read or was not loaded — an unverifiable model is not written. Settings are judged
 * on that same catalog entry, so one computer's allowed value is not written onto a computer that
 * lists different values for the model. Settings sent with the agent's default are judged against
 * that computer's current model; a default with no settings, and a custom command, still have
 * nothing to check. `catalogFor` receives the row, so one computer can be checked once per login.
 */
export const planRows = (plan: ApplyPlan, catalogFor: (item: RosterItem) => CatalogCheck): PlanRow[] => {
  const model = plan.model?.trim() ?? ''
  const options = Object.entries(plan.options)
  return plan.items.map((item) => {
    if (!needsCatalog(plan.backend, model || null, plan.options)) return { item, ok: true }
    const check = catalogFor(item)
    if (check.kind !== 'ok')
      return { item, ok: false, reason: notCheckable(item.computer.name), refusal: 'catalogUnreadable' } as const
    const entryId = model || check.catalog.currentModelId || ''
    const entry = entryId ? check.catalog.models.find((candidate) => candidate.id === entryId) : undefined
    if (!entry) {
      return model
        ? ({ item, ok: false, reason: notAvailable(item.computer.name), refusal: 'modelMissing' } as const)
        : ({ item, ok: false, reason: notCheckable(item.computer.name), refusal: 'catalogUnreadable' } as const)
    }
    return optionsFit(entry, options)
      ? { item, ok: true }
      : ({ item, ok: false, reason: settingsUnavailable(item.computer.name), refusal: 'settingsRejected' } as const)
  })
}

/** Every roster key, for `Select all`. */
export const allKeys = (items: RosterItem[]) => items.map((i) => refKey(i.ref))

/** Why an apply failed, in the five kinds the event counts: a refused row's kind, the host's guard, or the save. */
export type ApplyFailure = PlanRefusal | 'hostGuard' | 'saveError'

/**
 * An apply run's counts, in the shape the event sends. The five failure kinds always sum to `failed`: every refusal
 * is counted once in `failed` and once in its own kind, so the event can say what went wrong without any text, model
 * id or bot id.
 */
export type ApplyTally = {
  requested: number
  applied: number
  failed: number
  modelMissing: number
  catalogUnreadable: number
  settingsRejected: number
  hostGuard: number
  saveError: number
}

export const runTally = (requested: number): ApplyTally => ({
  requested,
  applied: 0,
  failed: 0,
  modelMissing: 0,
  catalogUnreadable: 0,
  settingsRejected: 0,
  hostGuard: 0,
  saveError: 0,
})

/** One save that landed. */
export const countApplied = (tally: ApplyTally) => {
  tally.applied += 1
}

/** One save that did not: counted in `failed` and in the kind that stopped it. */
export const countFailure = (tally: ApplyTally, failure: ApplyFailure) => {
  tally.failed += 1
  tally[failure] += 1
}

/**
 * How many checked bots the current filter hides. They stay checked, and Apply leaves them
 * alone: what the user cannot see is never written, so a filtered list can't hide a save.
 */
export const hiddenCheckedCount = (items: RosterItem[], shown: RosterItem[], checked: Set<string>) => {
  const shownKeys = new Set(shown.map((i) => refKey(i.ref)))
  const itemKeys = new Set(items.map((i) => refKey(i.ref)))
  return [...checked].filter((key) => !shownKeys.has(key) && itemKeys.has(key)).length
}

/**
 * What Apply will do right now: the bots it will write (`chosen` — checked and still visible),
 * how many checked bots the filter hides (`hiddenCount`, they stay checked and untouched), and
 * how many checked keys still exist (`totalChecked`, the toolbar count). Apply targets only
 * `chosen`, so a filtered list can't hide a save.
 */
export const applyTargets = (items: RosterItem[], shown: RosterItem[], checked: Set<string>) => ({
  chosen: filterChecked(shown, checked),
  hiddenCount: hiddenCheckedCount(items, shown, checked),
  totalChecked: filterChecked(items, checked).length,
})

/** `checked` with `key` turned on or off: a new set, so React state stays immutable. */
export const toggleKey = (checked: Set<string>, key: string, on: boolean) => {
  const next = new Set(checked)
  if (on) next.add(key)
  else next.delete(key)
  return next
}

/** One filterable provider in the Bot Manager's filter: a bot's agent (`backend`), named as hello names it. */
export type ProviderOption = { id: string; name: string }

/**
 * The providers the sheet's filter offers: every agent the given bots actually run, in hello's
 * order, each named the way the model picker names it. Derived from the bots present, never a
 * hardcoded list, so a roster on agents this build doesn't know still filters; an agent hello
 * doesn't name shows its raw id. `''` (all providers) is the menu's first row, not one of these.
 */
export const providerOptions = (items: RosterItem[], backends: ReadonlyArray<{ id: string; name: string }>): ProviderOption[] => {
  const present = new Set(items.map((i) => i.bot.backend))
  const known = backends.filter((b) => present.has(b.id)).map((b) => ({ id: b.id, name: b.name }))
  const missing = [...present].filter((id) => !backends.some((b) => b.id === id)).sort().map((id) => ({ id, name: id }))
  return [...known, ...missing]
}

/**
 * The patch a bulk pause or resume writes for one bot: `{ id, paused }` only, so a host that
 * understands the field keeps every other setting. A host that does not is answered by `pauseEcho`.
 */
export const pausePatch = (botId: string, paused: boolean) => ({ id: botId, paused })

/**
 * The bots a running batch may still write, in the order it captured them. A bot the filter
 * has hidden since that capture is dropped: the notice says no action touches a hidden bot,
 * and this write has not been sent yet.
 */
export const remainingTargets = (chosen: readonly RosterItem[], shownNow: ReadonlySet<string>): RosterItem[] =>
  chosen.filter((item) => shownNow.has(refKey(item.ref)))

/** How a pause the host already answered is counted. `show` is whether the row text is still drawn. */
export type PauseCount = { tally: 'applied' | 'failed'; show: boolean }

/**
 * A finished pause call. The host has already changed the bot, so the echo is counted even when
 * the sheet closed or the model pick changed while the request was in flight. `show` only decides
 * whether the row text is drawn.
 */
export const countedPause = (echoApplied: boolean, show: boolean): PauseCount => ({
  tally: echoApplied ? 'applied' : 'failed',
  show,
})

/** The segments of a bot's model path, joined the way the row already shows them. */
const PATH_SEPARATOR = '->'

/**
 * The agent's display name as the model picker names it, or null for one hello doesn't name — the
 * caller then shows the raw id it runs under, so a path is never left blank.
 */
export const agentNameOf = (backends: ReadonlyArray<{ id: string; name: string }>, backend: unknown): string | null =>
  typeof backend === 'string' ? (backends.find((b) => b.id === backend)?.name ?? null) : null

/** The computer this row's bot runs on, not the window's first computer. */
export const computerBackends = (
  item: { store: { hello?: { backends?: ReadonlyArray<{ id: string; name: string; loginProfiles?: { id: string; name: string | null }[] | null }> } | null } },
): ReadonlyArray<{ id: string; name: string; loginProfiles?: { id: string; name: string | null }[] | null }> =>
  item.store.hello?.backends ?? []

/**
 * The name of the login profile a bot runs on, when it picks one and the computer has named it. The
 * computer's own login has no id, so it adds nothing; a profile the computer hasn't named yet has
 * no name to show, so it adds nothing either.
 */
export const loginNameOf = (
  backends: ReadonlyArray<{ id: string; loginProfiles?: { id: string; name: string | null }[] | null }>,
  backend: unknown,
  loginProfile: unknown,
): string | null => {
  if (typeof backend !== 'string' || typeof loginProfile !== 'string' || !loginProfile) return null
  const profile = backends.find((b) => b.id === backend)?.loginProfiles?.find((p) => p.id === loginProfile)
  return profile?.name?.trim() || null
}

/**
 * The bot's current agent and model path for the row's small line, the way Ryan asked to see it:
 * agent, the login profile when it picks one, and the model id, joined with `->`.
 * `OpenCode->Work->opendesign/deepseek-v4.1-flash-eco`. A custom-command bot or one whose model
 * isn't readable shows nothing rather than a wrong guess.
 */
export const rowModelPath = (
  bot: { backend?: unknown; model?: unknown },
  agentName: string | null | undefined,
  loginName?: string | null,
): string | null => {
  if (typeof bot.backend !== 'string' || bot.backend === 'custom') return null
  if (typeof bot.model !== 'string' || !bot.model.trim()) return null
  const agent = agentName && agentName.trim() ? agentName.trim() : bot.backend
  const login = loginName?.trim()
  return [agent, ...(login ? [login] : []), bot.model.trim()].join(PATH_SEPARATOR)
}

/**
 * The model path as search words, so the manager can find a bot by what it runs: the whole path,
 * each segment of it, each part of the model id, the id read as words, and the model's own name
 * read as words. A custom-command bot or one whose model isn't readable adds nothing.
 */
export const modelPathWords = (
  bot: { backend?: unknown; model?: unknown },
  agentName: string | null | undefined,
  loginName?: string | null,
): string[] => {
  const path = rowModelPath(bot, agentName, loginName)
  if (!path) return []
  // The model id itself, not the tail of the joined path: a login's name may hold the separator.
  const model = (bot.model as string).trim()
  const asWords = (text: string) => text.replace(/[/_-]+/g, ' ')
  return [
    path,
    ...path.split(PATH_SEPARATOR),
    ...model.split('/'),
    asWords(model),
    ...(model.includes('/') ? [asWords(model.slice(model.indexOf('/') + 1))] : []),
  ]
}

/** The Bot Manager sheet's smallest usable size: every control reachable, list still scrollable. */
export const SHEET_MIN = { width: 480, height: 420 } as const

/** A dragged or re-measured sheet size, clamped to the bounds and the main window. */
export const clampSheetSize = (width: number, height: number, viewportWidth: number, viewportHeight: number) => {
  // 48 matches .modal-content's own max-width/max-height (calc(100vw|vh - 48px)).
  const maxWidth = Math.max(SHEET_MIN.width, viewportWidth - 48)
  const maxHeight = Math.max(SHEET_MIN.height, viewportHeight - 48)
  return {
    width: Math.round(Math.min(Math.max(width, SHEET_MIN.width), maxWidth)),
    height: Math.round(Math.min(Math.max(height, SHEET_MIN.height), maxHeight)),
  }
}

/**
 * The size a live drag proposes: anchored to the drag's START size, never to the already-updated
 * live one — pointer moves arrive as cumulative deltas, so anchoring to the live size would
 * compound every event (the runaway-drag bug the GLM review caught). The edge or corner decides
 * which way dragging grows it.
 */
export const resizeFrom = (
  kind: 'left' | 'right' | 'bottom' | 'corner-br' | 'corner-bl',
  anchor: { width: number; height: number },
  dx: number,
  dy: number,
  viewportWidth: number,
  viewportHeight: number,
) => {
  const growsWest = kind === 'left' || kind === 'corner-bl'
  const growsDown = kind !== 'left' && kind !== 'right'
  // The bottom handle is height only, the same rule stepSize already uses for its arrow keys.
  const growsAcross = kind !== 'bottom'
  return clampSheetSize(
    anchor.width + (growsAcross ? (growsWest ? -dx : dx) : 0),
    anchor.height + (growsDown ? dy : 0),
    viewportWidth,
    viewportHeight,
  )
}

/**
 * One arrow-key step on a handle: ±16 on the axis that handle advertises, nothing on the axis it
 * doesn't (the bottom handle says "height", so its Left/Right do nothing).
 */
export const stepSize = (
  kind: 'left' | 'right' | 'bottom' | 'corner-br' | 'corner-bl',
  size: { width: number; height: number },
  key: string,
  viewportWidth: number,
  viewportHeight: number,
) => {
  const d = 16
  const growsWest = kind === 'left' || kind === 'corner-bl'
  const horizontal = kind !== 'bottom'
  let dw = 0
  let dh = 0
  if (horizontal && key === 'ArrowLeft') dw = growsWest ? d : -d
  if (horizontal && key === 'ArrowRight') dw = growsWest ? -d : d
  if (key === 'ArrowDown') dh = d
  if (key === 'ArrowUp') dh = -d
  return clampSheetSize(size.width + dw, size.height + dh, viewportWidth, viewportHeight)
}

/**
 * The sheet's opening size, from what Ryan asked it to be: wide enough that the longest bot name
 * reads on one line (~460px of chrome around the text), never narrower than the original 640, and
 * about three quarters of the main window's height.
 */
export const defaultSheetSize = (names: string[], measure: (text: string) => number, viewportWidth: number, viewportHeight: number) => {
  const longest = names.reduce((max, name) => Math.max(max, measure(name)), 0)
  const wanted = Math.max(640, Math.round(460 + longest))
  const height = Math.round(Math.min(viewportHeight - 48, Math.max(560, viewportHeight * 0.78)))
  return clampSheetSize(wanted, height, viewportWidth, viewportHeight)
}
