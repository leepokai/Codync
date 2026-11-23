import type { ThreadGrouping } from '@shared/work'
import { DEFAULT_PANEL, type PanelLayout } from '../../shared/panel-layout.ts'
import type { AppLanguage } from './l10n'
import { useSyncExternalStore } from 'react'
import type { StarAsk } from './star-ask'
import type { ActiveDays, KofiAsk } from './kofi-ask'
import type { VoiceTap } from '../../shared/voice-shortcut'

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

/** `list` with `value` added, or taken out when it's there: a fold toggled. */
export function toggled(list: string[], value: string) {
  return list.includes(value) ? list.filter((v) => v !== value) : [...list, value]
}

export const TEXT_SIZES = [11, 12, 13, 14, 15, 16, 18]
export const DEFAULT_TEXT_SIZE = 12

/** The next larger (`up`) or smaller size for ⌘+ / ⌘−, staying at either end. */
export function stepTextSize(size: number, up: boolean) {
  return (up ? TEXT_SIZES.find((s) => s > size) : [...TEXT_SIZES].reverse().find((s) => s < size)) ?? size
}

export const prefs = {
  textSize: pref('conversationFontSize', DEFAULT_TEXT_SIZE),
  /** How the Bots list and the Threads list are laid out: under their computer's heading, and by status. */
  botsByComputer: pref('botsByComputer', true),
  botsByStatus: pref('botsByStatus', false),
  /** Keep the lead bot pinned at the very top of the Bots list on this device, ahead of the activity bumping. */
  freezeLeadAtTop: pref('freezeLeadAtTop', false),
  /** How the Threads list groups what doesn't need the user: by folder (and routine), by bot, or not at all. */
  threadsGroup: pref<ThreadGrouping>('threadsGroup', 'folder'),
  /** How the Threads list orders the rows of each group, beside how it groups them. */
  threadsByStatus: pref('threadsByStatus', false),
  threadsByActivity: pref('threadsByActivity', false),
  /** The list beside the rail (the bots, the threads) is folded away. */
  listHidden: pref('listHidden', false),
  sidebarWidth: pref('desktopSidebarWidth', 266),
  detailsPanelWidth: pref('detailsPanelWidth', 400),
  /** The chat a workflow is built in shows beside the canvas (WorkflowChat): open until closed. */
  workflowChatOpen: pref('workflowChatOpen', true),
  /** The panel beside a conversation: its panes, their tabs and the one each shows, the same for every bot. Read
   * through `panelLayout`, which makes what is stored safe to draw. */
  panelLayout: pref<PanelLayout>('panelLayout', DEFAULT_PANEL),
  hiddenComputers: pref('hiddenComputers', ''),
  computerOrder: pref<string[]>('computerOrder', []),
  collapsedComputers: pref<string[]>('collapsedComputerSections', []),
  /** Folded roster sections, by name: on this device only. */
  rosterSectionOrder: pref<Record<string, string[]>>('rosterSectionOrder', {}),
  collapsedRosterSections: pref<string[]>('collapsedRosterSections', []),
  /** Folded groups of the Threads list, by the group's key: on this device only. */
  foldedThreadSections: pref<string[]>('foldedThreadSections', []),
  /** Folded computers of the Workflows list, by computer id: on this device only. */
  foldedWorkflowComputers: pref<string[]>('foldedWorkflowComputers', []),
  /** The workflow canvas's bottom bars (zoom and tools) are folded into one button: on this device only. */
  workflowToolsFolded: pref('workflowToolsFolded', false),
  /** Folded sections of a settings form (a bot's), by the section's key: on this device only. */
  foldedCardSections: pref<string[]>('foldedCardSections', []),
  usageIconStyle: pref<'character' | 'original'>('usageIconStyle', 'character'),
  /** The roster marks each bot with its agent's logo while the bots run more than one agent. */
  agentLogos: pref('showAgentLogos', true),
  onboardingDone: pref('macAccountOnboardingCompleted', false),
  /** Chose to go on without signing in; signing in ends it. */
  withoutAccount: pref('withoutAccount', false),
  computerAccessSetup: pref<'ready' | 'later' | null>('computerAccessSetup', null),
  starAsk: pref<StarAsk>('githubStarAsk', { kind: 'never' }),
  /** Prompts this computer delivered, voice calls aside; they bring the second star ask. */
  promptsSent: pref('promptsSent', 0),
  /** Days this computer sent a prompt on, voice calls and threads included; they bring the Ko-fi invitation. */
  activeDays: pref<ActiveDays>('activeDays', { count: 0, last: '' }),
  kofiAsk: pref<KofiAsk>('kofiAsk', { asked: 0, day: 0 }),
  appLanguage: pref<AppLanguage>('appLanguage', 'system'),
  /** The working pill unfolds the turn's thinking and steps; off keeps the chat to its messages. */
  thinkingDetails: pref('showThinkingDetails', false),
  /** The voice input shortcut's tap after its double tap: sends, or stops and keeps the words in the box. */
  voiceShortcutTap: pref<VoiceTap>('voiceShortcutTap', 'send'),
  /** The version this device last opened on, for What's new; `null` before it ever recorded one. */
  whatsNewSeen: pref<string | null>('whatsNewSeen', null),
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
