import type { BotDraft } from './models'

const same = (a: unknown, b: unknown) => JSON.stringify(a) === JSON.stringify(b)

/**
 * `draft` with each field it hasn't changed from `base` (the bot as the form loaded it) taken from `fresh` (the bot
 * as the host has it now), so saving the form keeps a change made elsewhere: a bot that manages the team, another
 * device. The same draft when nothing moves.
 */
export function rebase(draft: BotDraft, base: BotDraft, fresh: BotDraft): BotDraft {
  const keys = new Set([...Object.keys(draft), ...Object.keys(fresh)] as (keyof BotDraft)[])
  const moved = [...keys].filter((k) => same(draft[k], base[k]) && !same(draft[k], fresh[k]))
  return moved.length ? { ...draft, ...Object.fromEntries(moved.map((k) => [k, fresh[k]])) } : draft
}
