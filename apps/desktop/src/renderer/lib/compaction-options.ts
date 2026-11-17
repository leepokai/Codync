import type { CompactionSizes, CompactionThreshold } from '@shared/models'
import type { MenuItem } from '../components/Menu'

const PRESETS: { id: Exclude<CompactionThreshold, number>; title: string }[] = [
  { id: 'off', title: 'Auto (default)' },
  { id: 'early', title: 'Early' },
  { id: 'nearLimit', title: 'Near the limit' },
]

/** A size as the apps show it: thousands of tokens, `120K`. */
export const sizeLabel = (tokens: number) => `${Math.round(tokens / 1000)}K`

/** The pill's text: a preset's English phrase (translated where it shows), or a size as it is. */
export function compactionLabel(threshold: CompactionThreshold): { title: string; verbatim: boolean } {
  if (typeof threshold === 'number') return { title: sizeLabel(threshold), verbatim: true }
  return { title: PRESETS.find((p) => p.id === threshold)?.title ?? PRESETS[0].title, verbatim: false }
}

/** A typed size as one the window takes: to the nearest step, within `min` and `max`. */
export function fitSize(sizes: CompactionSizes, tokens: number): number {
  const step = Math.max(sizes.step, 1)
  return Math.min(Math.max(Math.floor((tokens + step / 2) / step) * step, sizes.min), sizes.max)
}

/** What the user typed, in thousands of tokens (`350`, `350K`), as tokens; null when it isn't a size. */
export function parseTypedSize(text: string): number | null {
  const k = Number(text.trim().replace(/k$/i, ''))
  return text.trim() && Number.isFinite(k) && k > 0 ? Math.round(k * 1000) : null
}

/**
 * The presets, then "Set a size" opening the window's few listed sizes and "Custom size…" (`custom`); before the
 * window is known, a line saying when they come.
 */
export function compactionMenu(selection: CompactionThreshold, sizes: CompactionSizes | null,
  pick: (threshold: CompactionThreshold) => void, custom: () => void): MenuItem[] {
  const presets: MenuItem[] = PRESETS.map((p) => ({ title: p.title, selected: selection === p.id, action: () => pick(p.id) }))
  if (!sizes) return [...presets, { title: "Sizes show after the bot's next reply", disabled: true, divider: true, action: () => {} }]
  const listed: MenuItem[] = sizes.presets.map((n) => ({ title: sizeLabel(n), verbatim: true, selected: selection === n, action: () => pick(n) }))
  const own = typeof selection === 'number' && !sizes.presets.includes(selection)
  return [...presets, { title: 'Set a size', divider: true, submenu: [...listed, { title: 'Custom size…', selected: own, action: custom }] }]
}
