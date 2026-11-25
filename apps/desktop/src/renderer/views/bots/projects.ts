import type { BotDraft, WorktreeMode } from '../../../shared/models.ts'

/** `path` added last (the default when it's the first); one already there stays where it is. */
export const withProject = (d: BotDraft, path: string): BotDraft =>
  ({ ...d, projects: d.projects?.includes(path) ? d.projects : [...(d.projects ?? []), path] })

export const withoutProject = (d: BotDraft, path: string): BotDraft => ({ ...d, projects: (d.projects ?? []).filter((p) => p !== path) })

/** `path` as the default: moved first. */
export const withDefaultProject = (d: BotDraft, path: string): BotDraft =>
  ({ ...d, projects: [path, ...(d.projects ?? []).filter((p) => p !== path)] })

/** A typed shared folder as the host takes it, a plain path inside the project (`node_modules`, `.cache/pip`): trimmed,
 * no `./` or trailing slash; null for an absolute path, one with `..`, or nothing. */
export function sharedFolderName(typed: string): string | null {
  const text = typed.trim()
  const parts = text.split(/[\\/]/).filter((p) => p && p !== '.')
  return /^([\\/]|[A-Za-z]:)/.test(text) || !parts.length || parts.includes('..') ? null : parts.join('/')
}

/** `name` added to `project`'s shared folders (one already there stays). */
export function withSharedFolder(d: BotDraft, project: string, name: string): BotDraft {
  const list = d.sharedFolders?.[project] ?? []
  return list.includes(name) ? d : { ...d, sharedFolders: { ...d.sharedFolders, [project]: [...list, name] } }
}

/** `name` taken off `project`'s shared folders; a project left with none leaves the map. */
export function withoutSharedFolder(d: BotDraft, project: string, name: string): BotDraft {
  const { [project]: list = [], ...others } = d.sharedFolders ?? {}
  const left = list.filter((n) => n !== name)
  return { ...d, sharedFolders: left.length ? { ...others, [project]: left } : others }
}

/** The worktree modes, in the order Settings lists them, each with its line. */
export const WORKTREE_MODES: { id: WorktreeMode; label: string; detail: string }[] = [
  { id: 'off', label: 'Off', detail: 'Threads work in the project itself' },
  { id: 'sideBySide', label: 'Side by side', detail: 'A thread gets its own worktree when another thread works in the same repository' },
  { id: 'always', label: 'Always', detail: "Every thread works in a worktree of its own; your checkout is never touched" },
]
