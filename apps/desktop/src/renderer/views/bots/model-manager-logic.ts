import type { RosterItem } from '../../store/app-model'
import { refKey } from '../../store/bot-ref.ts'
import { trNow } from '../../lib/l10n.ts'
import { MODEL_PATH_JOIN } from '../../../shared/model-path.ts'
import type { AgentModels } from './agent-models'

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

/** What a computer's catalog check found: its catalog, a read failure, or nothing to check against. */
export type CatalogCheck = { kind: 'ok'; catalog: AgentModels } | { kind: 'error' } | { kind: 'none' }

/**
 * Which bots the plan can save to, decided before anything is written.
 *
 * With a model chosen, every computer's catalog must have verified it: a computer whose agent
 * doesn't offer the model is skipped with a reason, and so is a computer whose catalog could not be
 * read or was not loaded — an unverifiable model is not written. Only the agent's default (no
 * model) and a custom command, which have nothing to check against, are left to the host.
 *
 * Known gap, left visible on purpose: `catalogFor` is keyed by computer alone, so the catalog read
 * carries no login and is therefore the computer's DEFAULT login. The patch this plan builds
 * deliberately omits `loginProfile`, so each bot keeps its own — which means a bot running a named
 * login can pass this check against a catalog it cannot actually run, and the host then refuses the
 * save. The two halves disagree: the check promises a row the save cannot deliver. The fix is to key
 * the check by (computer, agent, the bot's own login) rather than one catalog per computer.
 */
export const planRows = (plan: ApplyPlan, catalogFor: (computerId: string) => CatalogCheck): PlanRow[] => {
  const model = plan.model?.trim() ?? ''
  return plan.items.map((item) => {
    if (!model || plan.backend === 'custom') return { item, ok: true }
    const check = catalogFor(item.ref.computerId)
    if (check.kind === 'ok') {
      const available = check.catalog.models.some((m) => m.id === model)
      return available ? { item, ok: true } : { item, ok: false, reason: notAvailable(item.computer.name) }
    }
    return { item, ok: false, reason: notCheckable(item.computer.name) }
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
 * The patch a bulk pause or resume writes for one bot: the same `updateBot` path the roster's own
 * pause toggle uses, and more minimal — `{ id, paused }` only — so the host's patched merge keeps
 * every other setting the bot has.
 */
export const pausePatch = (botId: string, paused: boolean) => ({ id: botId, paused })

/**
 * The model path as search words, so the manager can find a bot by what it runs: the whole path,
 * each segment of it, each part of the model id (`opendesign`, `deepseek-v4.1-flash-eco`), the
 * id read as words (`opendesign deepseek v4.1 flash eco`) and the model's own name read as words
 * (`deepseek v4.1 flash eco`), which is how a model is usually typed. A custom-command bot or one
 * whose model isn't readable adds nothing, so it is found by its name alone. Every word is added
 * as it is: the matcher is a case-insensitive substring test, so a phrase matches without any
 * splitting on the query's side.
 *
 * `path` is the row's own path — the one `botModelPath` builds from the roster row, which is also
 * the one the row prints — so what is searched is what the list shows, from one string.
 */
export const modelPathWords = (bot: { model?: unknown }, path: string | null): string[] => {
  if (!path) return []
  // The model id itself, not the tail of the joined path: a login's name may hold the separator.
  const model = typeof bot.model === 'string' ? bot.model.trim() : ''
  const asWords = (text: string) => text.replace(/[/_-]+/g, ' ')
  return [
    path,
    ...path.split(MODEL_PATH_JOIN),
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
  // The bottom handle says "height": horizontal pointer drift never changes the width.
  const widthDx = kind === 'bottom' ? 0 : growsWest ? -dx : dx
  return clampSheetSize(
    anchor.width + widthDx,
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
  const vertical = kind !== 'left' && kind !== 'right'
  let dw = 0
  let dh = 0
  if (horizontal && key === 'ArrowLeft') dw = growsWest ? d : -d
  if (horizontal && key === 'ArrowRight') dw = growsWest ? -d : d
  if (vertical && key === 'ArrowDown') dh = d
  if (vertical && key === 'ArrowUp') dh = -d
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
