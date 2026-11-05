import type { AppLanguage } from './l10n'
import { useSyncExternalStore } from 'react'
import type { StarAsk } from './star-ask'

/** A stored value, or the fallback when it's missing, unreadable or of another kind (a `null` where a string
 * belongs crashed the chat window). A `null` fallback takes any value. */
function stored<T>(raw: string | null, fallback: T): T {
  if (raw === null) return fallback
  try {
    const value: unknown = JSON.parse(raw)
    const sameKind = value !== null && typeof value === typeof fallback && Array.isArray(value) === Array.isArray(fallback)
    return fallback === null || sameKind ? (value as T) : fallback
  } catch {
    return fallback
  }
}

/** A per-window preference in localStorage (kit's `@AppStorage`), observable by views. */
export function pref<T>(key: string, fallback: T) {
  const listeners = new Set<() => void>()
  let value: T = (() => {
    try {
      return stored(localStorage.getItem(key), fallback)
    } catch {
      return fallback
    }
  })()
  window.addEventListener('storage', (e) => {
    if (e.key !== key) return
    value = stored(e.newValue, fallback)
    for (const l of listeners) l()
  })
  return {
    get: () => value,
    set(next: T) {
      value = next
      try {
        localStorage.setItem(key, JSON.stringify(next))
      } catch {}
      for (const l of listeners) l()
    },
    subscribe(l: () => void) {
      listeners.add(l)
      return () => void listeners.delete(l)
    },
  }
}

export type Pref<T> = ReturnType<typeof pref<T>>

export function usePref<T>(p: Pref<T>): [T, (v: T) => void] {
  const value = useSyncExternalStore(p.subscribe, p.get)
  return [value, p.set]
}

export const TEXT_SIZES = [11, 12, 13, 14, 15, 16, 18]
export const DEFAULT_TEXT_SIZE = 12

/** The next larger (`up`) or smaller size for ⌘+ / ⌘−, staying at either end. */
export function stepTextSize(size: number, up: boolean) {
  return (up ? TEXT_SIZES.find((s) => s > size) : [...TEXT_SIZES].reverse().find((s) => s < size)) ?? size
}

export const prefs = {
  textSize: pref('conversationFontSize', DEFAULT_TEXT_SIZE),
  sidebarCompact: pref('sidebarCompact', false),
  sidebarWidth: pref('desktopSidebarWidth', 266),
  detailsPanelWidth: pref('detailsPanelWidth', 400),
  hiddenComputers: pref('hiddenComputers', ''),
  computerOrder: pref<string[]>('computerOrder', []),
  collapsedComputers: pref<string[]>('collapsedComputerSections', []),
  /** Folded roster sections, by name: on this device only. */
  rosterSectionOrder: pref<Record<string, string[]>>('rosterSectionOrder', {}),
  collapsedRosterSections: pref<string[]>('collapsedRosterSections', []),
  usageIconStyle: pref<'character' | 'original'>('usageIconStyle', 'character'),
  /** The roster marks each bot with its agent's logo while the bots run more than one agent. */
  agentLogos: pref('showAgentLogos', true),
  onboardingDone: pref('macAccountOnboardingCompleted', false),
  computerAccessSetup: pref<'ready' | 'later' | null>('computerAccessSetup', null),
  starAsk: pref<StarAsk>('githubStarAsk', { kind: 'never' }),
  /** Prompts this computer delivered, voice calls aside; they bring the second star ask. */
  promptsSent: pref('promptsSent', 0),
  appLanguage: pref<AppLanguage>('appLanguage', 'system'),
  /** The working pill unfolds the turn's thinking and steps; off keeps the chat to its messages. */
  thinkingDetails: pref('showThinkingDetails', false),
  extensionsMovedTipSeen: pref('extensionsMovedTipSeen', false),
}

/** The Mac preference supplies one scale for conversation text and app chrome. */
export function applyTextSize(size: number) {
  const clamped = Number.isFinite(size) ? Math.min(Math.max(size, 11), 18) : DEFAULT_TEXT_SIZE
  document.documentElement.style.setProperty('--scale', String(clamped / DEFAULT_TEXT_SIZE))
  document.documentElement.style.setProperty('--conversation', `${clamped}px`)
  // Match the reference chat at the selected size; keep larger text an explicit preference.
  document.documentElement.style.setProperty('--chat', `${clamped}px`)
  document.documentElement.style.setProperty('--chat-scale', String(clamped / DEFAULT_TEXT_SIZE))
}

prefs.textSize.subscribe(() => applyTextSize(prefs.textSize.get()))
