import type { BotDraft, WorktreeMode } from '../../../shared/models.ts'

/** `path` added last (the default when it's the first); one already there stays where it is. */
export const withProject = (d: BotDraft, path: string): BotDraft =>
  ({ ...d, projects: d.projects?.includes(path) ? d.projects : [...(d.projects ?? []), path] })

export const withoutProject = (d: BotDraft, path: string): BotDraft => ({ ...d, projects: (d.projects ?? []).filter((p) => p !== path) })

/** `path` as the default: moved first. */
export const withDefaultProject = (d: BotDraft, path: string): BotDraft =>
  ({ ...d, projects: [path, ...(d.projects ?? []).filter((p) => p !== path)] })

/** The worktree modes, in the order Settings lists them, each with its line. */
export const WORKTREE_MODES: { id: WorktreeMode; label: string; detail: string }[] = [
  { id: 'off', label: 'Off', detail: 'Threads work in the project itself' },
  { id: 'sideBySide', label: 'Side by side', detail: 'A thread gets its own worktree when another thread works in the same repository' },
  { id: 'always', label: 'Always', detail: "Every thread works in a worktree of its own; your checkout is never touched" },
]
