import { isDark } from '../../lib/theme'
export {
  isLimit, parseReset, providerRegistry, resetDate, resetDescription, spentNote, spentWindow, tightest,
  untilText, usageStatus, usedText, windowReading, windowTitle,
} from '../../lib/usage-presentation'

/** Claude keeps its orange; everyone else is ink, like the rest of the app. */
export function providerTint(id: string) {
  return id === 'claude' ? '#D97757' : isDark() ? '#FFFFFF' : '#000000'
}

/** CSS color for views (follows the scheme by itself). */
export const providerTintCSS = (id: string) => (id === 'claude' ? '#D97757' : 'var(--accent)')

/** The character that stands for this provider. */
export const mascotShape = (id: string) => (id === 'codex' ? 'hex' : 'blob')
