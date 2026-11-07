import type { CompactionSizes, CompactionThreshold } from '@shared/models'
import type { MenuItem } from '../components/Menu'

const PRESETS: { id: Exclude<CompactionThreshold, number>; title: string }[] = [
  { id: 'early', title: 'Early (default)' },
  { id: 'nearLimit', title: 'Near the limit' },
  { id: 'off', title: 'Only when full' },
]

/** A size as the apps show it: thousands of tokens, `120K`. */
export const sizeLabel = (tokens: number) => `${Math.round(tokens / 1000)}K`

/** The pill's text: a preset's English phrase (translated where it shows), or a size as it is. */
export function compactionLabel(threshold: CompactionThreshold): { title: string; verbatim: boolean } {
  if (typeof threshold === 'number') return { title: sizeLabel(threshold), verbatim: true }
  return { title: PRESETS.find((p) => p.id === threshold)?.title ?? PRESETS[0].title, verbatim: false }
}

/** Every size the window allows, smallest first. */
export function compactionSizes(sizes: CompactionSizes): number[] {
  if (sizes.step <= 0) return []
  const out: number[] = []
  for (let n = sizes.min; n <= sizes.max; n += sizes.step) out.push(n)
  return out
}

/** The presets, then "Set a size" opening the sizes; before the window is known, a line saying when they come. */
export function compactionMenu(selection: CompactionThreshold, sizes: CompactionSizes | null, pick: (threshold: CompactionThreshold) => void): MenuItem[] {
  const presets: MenuItem[] = PRESETS.map((p) => ({ title: p.title, selected: selection === p.id, action: () => pick(p.id) }))
  if (!sizes) return [...presets, { title: "Sizes show after the bot's next reply", disabled: true, divider: true, action: () => {} }]
  const submenu: MenuItem[] = compactionSizes(sizes).map((n) => ({ title: sizeLabel(n), verbatim: true, selected: selection === n, action: () => pick(n) }))
  return [...presets, { title: 'Set a size', divider: true, submenu }]
}
