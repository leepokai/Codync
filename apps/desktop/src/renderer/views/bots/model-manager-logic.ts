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
export const notAvailable = (computer: string) => trNow('Not available on {computer}').replace('{computer}', computer)

/**
 * Which bots the plan can save to, decided before anything is written.
 *
 * A bot whose computer advertises a catalog without the model is skipped with a reason. A computer we
 * have no catalog for (offline, still loading) is left to the host: refusing the save there would make
 * an unreachable check look like a missing model. A custom command has no catalog and takes any model
 * ID, so it is never checked.
 */
export const planRows = (plan: ApplyPlan, catalogFor: (computerId: string) => AgentModels | null): PlanRow[] => {
  const model = plan.model?.trim() ?? ''
  return plan.items.map((item) => {
    if (!model || plan.backend === 'custom') return { item, ok: true }
    const catalog = catalogFor(item.ref.computerId)
    if (!catalog) return { item, ok: true }
    const available = catalog.models.some((m) => m.id === model)
    return available ? { item, ok: true } : { item, ok: false, reason: notAvailable(item.computer.name) }
  })
}

/** Every roster key, for `Select all`. */
export const allKeys = (items: RosterItem[]) => items.map((i) => refKey(i.ref))

/** `checked` with `key` turned on or off: a new set, so React state stays immutable. */
export const toggleKey = (checked: Set<string>, key: string, on: boolean) => {
  const next = new Set(checked)
  if (on) next.add(key)
  else next.delete(key)
  return next
}
