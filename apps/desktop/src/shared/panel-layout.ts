import { isBelow } from './compat.ts'

/** One pane of the panel beside a conversation: the views open as its tabs, and the one it shows (null: its chooser). */
export interface PanelPane {
  tabs: string[]
  shown: string | null
}

/** The panel on this device: one pane, or two, the second under the first. */
export type PanelLayout = PanelPane[]

/** Every view of a bot, in the order the chooser lists them, each with the key that opens it there. */
export const PANEL_VIEWS = [
  { id: 'details', label: 'Details', icon: 'checklist', key: 'd' },
  { id: 'computer', label: 'Computer', icon: 'display', key: 'c' },
  { id: 'library', label: 'Library', icon: 'folder', key: 'l' },
  { id: 'memory', label: 'Memory', icon: 'brain', key: 'm' },
  { id: 'settings', label: 'Settings', icon: 'gearshape', key: 's' },
  { id: 'browser', label: 'Browser', icon: 'globe', key: 'b' },
  { id: 'terminal', label: 'Terminal', icon: 'terminal', key: 't' },
] as const

export type PanelView = (typeof PANEL_VIEWS)[number]['id']

/** The first host with a terminal in the bot's folder (`botTerminal`). */
export const TERMINAL_HOST = '2.52.0'
export const hasTerminal = (hostVersion: string | undefined) => !!hostVersion && !isBelow(hostVersion, TERMINAL_HOST)

/** The views a pane opens a new tab of each time one is picked (several web pages, several shells): `<view>:<id>`. */
export const MULTI_VIEWS = ['browser', 'terminal'] as const satisfies readonly PanelView[]
export type MultiView = (typeof MULTI_VIEWS)[number]
const isMultiView = (view: string): view is MultiView => (MULTI_VIEWS as readonly string[]).includes(view)
/** The view a tab of a multi view is one of ('browser' for `browser:k3x9`), else null. */
export const multiView = (tab: string): MultiView | null => MULTI_VIEWS.find((v) => tab.startsWith(`${v}:`) && tab.length > v.length + 1) ?? null

/** The tabs of multi views among `tabs`, each numbered among its view's (`n`: 2 for "Terminal 2"). */
export function multiTabs(tabs: readonly string[]): { id: string; view: MultiView; n: number }[] {
  const counts = new Map<MultiView, number>()
  return tabs.flatMap((id) => {
    const view = multiView(id)
    if (!view) return []
    const n = (counts.get(view) ?? 0) + 1
    counts.set(view, n)
    return [{ id, view, n }]
  })
}

/** The tab of a task's thread: it opens beside the bot's views, from a row of Details. */
export const threadTab = (taskId: string) => `thread:${taskId}`
export const isThreadTab = (tab: string) => tab.startsWith('thread:')

/** A new panel: one pane on its chooser, where every view opens from. */
export const DEFAULT_PANEL: PanelLayout = [{ tabs: [], shown: null }]

/** `pane` without `view`: the tab beside it takes its place when it was the one shown, the chooser with none left. */
function without(pane: PanelPane, view: string): PanelPane {
  const at = pane.tabs.indexOf(view)
  if (at < 0) return pane
  const tabs = pane.tabs.filter((t) => t !== view)
  return { tabs, shown: pane.shown === view ? (tabs[Math.min(at, tabs.length - 1)] ?? null) : pane.shown }
}

/** Shows `view` in pane `at`, as a new tab after its others unless it has one there. A view has one tab: it leaves the
 * other pane. Without `at`: in the pane that has its tab, else the first. A multi view (`MULTI_VIEWS`) always opens a
 * new tab of its own. */
export function showView(layout: PanelLayout, picked: string, at?: number): PanelLayout {
  const view = isMultiView(picked) ? `${picked}:${crypto.randomUUID().slice(0, 8)}` : picked
  const has = layout.findIndex((p) => p.tabs.includes(view))
  const target = at ?? Math.max(has, 0)
  return layout.map((pane, i) => (i === target ? { tabs: pane.tabs.includes(view) ? pane.tabs : [...pane.tabs, view], shown: view } : without(pane, view)))
}

/** Closes `view`'s tab in pane `at`. */
export const closeView = (layout: PanelLayout, at: number, view: string): PanelLayout => layout.map((pane, i) => (i === at ? without(pane, view) : pane))

/** Pane `at` shows its chooser; its tabs stay. */
export const showChooser = (layout: PanelLayout, at: number): PanelLayout => layout.map((pane, i) => (i === at ? { ...pane, shown: null } : pane))

/** One pane becomes two (the new one under it, on its chooser); two become one, holding the tabs of both. */
export function toggleSplit(layout: PanelLayout): PanelLayout {
  const [first, second] = layout
  if (!first) return DEFAULT_PANEL
  if (!second) return [first, { tabs: [], shown: null }]
  return [{ tabs: [...first.tabs, ...second.tabs], shown: first.shown ?? second.shown }]
}

/** The layout as stored, made safe to draw: one or two panes, each tab once and only those it `keeps` (a view this
 * computer has no page for, a thread that is gone or another bot's are left out; a multi view's tab is kept with its
 * view, and the bare view is no tab), a pane showing one of its own tabs or its chooser. A pane whose thread was left
 * out shows its first tab instead. */
export function panelLayout(raw: unknown, keeps: (tab: string) => boolean): PanelLayout {
  const seen = new Set<string>()
  const panes = (Array.isArray(raw) ? raw : []).slice(0, 2).map((pane: Partial<PanelPane> | null): PanelPane => {
    const tabs = (Array.isArray(pane?.tabs) ? pane.tabs : []).filter((t) => typeof t === 'string' && !isMultiView(t) && keeps(multiView(t) ?? t) && !seen.has(t) && !!seen.add(t))
    const gone = typeof pane?.shown === 'string' && isThreadTab(pane.shown)
    return { tabs, shown: tabs.find((t) => t === pane?.shown) ?? (gone ? (tabs[0] ?? null) : null) }
  })
  return panes.length ? panes : panelLayout(DEFAULT_PANEL, keeps)
}

/** The address a typed line previews, or null when it is not a web address: `localhost:3000` and `example.com` get
 * their scheme (plain http for this computer, https for the rest), and only http and https pages show. The app's own
 * page (`own`, its origin) is refused: framed in itself it would hold the app's access. */
export function previewAddress(typed: string, own: string): string | null {
  const text = typed.trim()
  if (!text) return null
  const local = /^(localhost|127\.0\.0\.1|\[::1\])(:|\/|$)/.test(text)
  try {
    const url = new URL(/^[a-z][a-z0-9+.-]*:\/\//i.test(text) ? text : `${local ? 'http' : 'https'}://${text}`)
    return (url.protocol === 'http:' || url.protocol === 'https:') && url.origin !== own ? url.href : null
  } catch {
    return null
  }
}
