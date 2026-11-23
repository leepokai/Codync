import type { RosterItem } from '../../store/app-model'
import { refKey } from '../../store/bot-ref.ts'
import { trNow } from '../../lib/l10n.ts'
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
