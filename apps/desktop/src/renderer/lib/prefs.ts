import { useSyncExternalStore } from 'react'
import type { StarAsk } from './star-ask'

/** A per-window preference in localStorage (kit's `@AppStorage`), observable by views. */
export function pref<T>(key: string, fallback: T) {
  const listeners = new Set<() => void>()
  let value: T = (() => {
    try {
      const raw = localStorage.getItem(key)
      return raw === null ? fallback : (JSON.parse(raw) as T)
    } catch {
      return fallback
    }
  })()
  window.addEventListener('storage', (e) => {
    if (e.key !== key) return
    try {
      value = e.newValue === null ? fallback : (JSON.parse(e.newValue) as T)
    } catch {
      value = fallback
    }
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
  hiddenComputers: pref('hiddenComputers', ''),
  computerOrder: pref<string[]>('computerOrder', []),
  collapsedComputers: pref<string[]>('collapsedComputerSections', []),
  usageIconStyle: pref<'character' | 'original'>('usageIconStyle', 'character'),
  onboardingDone: pref('macAccountOnboardingCompleted', false),
  computerAccessSetup: pref<'ready' | 'later' | null>('computerAccessSetup', null),
  starAsk: pref<StarAsk>('githubStarAsk', { kind: 'never' }),
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
