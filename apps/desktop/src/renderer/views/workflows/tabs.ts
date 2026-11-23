// Which workflows are open as tabs (the list itself is a pref, workflowTabs in pins.ts).

const MAX_TABS = 12

/** Adds a tab for `id` at the end, unless it has one; past the limit the oldest tab goes. */
export function openTab(tabs: string[], id: string) {
  return tabs.includes(id) ? tabs : [...tabs, id].slice(-MAX_TABS)
}

/** The tabs without `id`, and the one to show if `id` was shown: the tab after it, or else the one before. */
export function closeTab(tabs: string[], id: string): { tabs: string[]; next: string | null } {
  const at = tabs.indexOf(id)
  const rest = tabs.filter((item) => item !== id)
  return { tabs: rest, next: rest[Math.min(Math.max(at, 0), rest.length - 1)] ?? null }
}
