// The checklist a bot keeps for its work (its `plan` entries: an ACP agent's own plan, or `update_plan` from the
// `chat` server), shown above the message box. Plain functions, so they can be tested without a window.
import type { Entry, PlanItem } from '../../../shared/models.ts'

export interface TaskChecklist {
  items: PlanItem[]
  done: number
}

/**
 * The checklist of the chat's latest turn: the bot's last `plan` entry, while the bot works or something on it
 * is still open. None for an older turn's plan (the next turn didn't write one), a finished plan once the bot
 * is idle, or a plan with nothing in it. The full conversation keeps every plan.
 */
export function currentChecklist(entries: Entry[], working: boolean): TaskChecklist | null {
  let plan: Entry | null = null
  let turn = 0
  for (const e of entries) {
    if (e.turn > turn) turn = e.turn
    if (e.kind === 'plan' && (!plan || e.seq > plan.seq)) plan = e
  }
  const items = plan?.data.entries ?? []
  if (!plan || plan.turn !== turn || !items.length) return null
  const done = items.filter((i) => i.status === 'completed').length
  if (done === items.length && !working) return null
  return { items, done }
}
