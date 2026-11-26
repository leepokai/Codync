import type { RosterItem } from '../../store/app-model'
import { refKey } from '../../store/bot-ref.ts'
import { trNow } from '../../lib/l10n.ts'
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

/** One bot's fate: `ok` rows are saved, the rest carry the reason they were skipped. */
export type PlanRow = { item: RosterItem; ok: boolean; reason?: string }

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
    if (check.kind !== 'ok') return { item, ok: false, reason: notCheckable(item.computer.name) }
    const entryId = model || check.catalog.currentModelId || ''
    const entry = entryId ? check.catalog.models.find((candidate) => candidate.id === entryId) : undefined
    if (!entry) {
      return { item, ok: false, reason: model ? notAvailable(item.computer.name) : notCheckable(item.computer.name) }
    }
    return optionsFit(entry, options) ? { item, ok: true } : { item, ok: false, reason: settingsUnavailable(item.computer.name) }
  })
}

/** Every roster key, for `Select all`. */
export const allKeys = (items: RosterItem[]) => items.map((i) => refKey(i.ref))

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

/**
 * The patch a bulk pause or resume writes for one bot: `{ id, paused }` only, so a host that
 * understands the field keeps every other setting. A host that does not is answered by `pauseEcho`.
 */
export const pausePatch = (botId: string, paused: boolean) => ({ id: botId, paused })

/**
 * The bot's current agent -> model path for the row's small line, the way Ryan asked to see it:
 * `opencode->open-design/deepseek 4.1 flash`. The agent shows the display name the picker knows
 * (else its raw id); the model shows its id. A custom-command bot or one whose model isn't
 * readable shows nothing rather than a wrong guess.
 */
export const rowModelPath = (bot: { backend?: unknown; model?: unknown }, agentName: string | null | undefined): string | null => {
  if (typeof bot.backend !== 'string' || bot.backend === 'custom') return null
  if (typeof bot.model !== 'string' || !bot.model.trim()) return null
  const agent = agentName && agentName.trim() ? agentName.trim() : bot.backend
  return `${agent}->${bot.model.trim()}`
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
