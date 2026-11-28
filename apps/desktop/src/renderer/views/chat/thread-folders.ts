import type { WorktreeMode } from '../../../shared/models.ts'
import type { ThreadPlace, ThreadSetup } from '../../../shared/work.ts'

/** The folder a new thread with `bot` works in when none is picked: its default project, else its own folder. */
export const defaultFolder = (bot: { cwd: string; projects: string[] }) => bot.projects[0] ?? bot.cwd

/** The folders the new-thread chip lists for `bot`: its projects (the default first), its own folder, then the recent
 * folders not listed yet (`recent` true). `cwd` is what the thread sends: nothing for the default, which the host
 * picks itself. */
export function threadFolders(bot: { cwd: string; projects: string[] }, recent: string[]) {
  const listed = [...bot.projects, bot.cwd]
  return [...listed, ...recent.filter((path) => !listed.includes(path))].map((path, at) => ({
    path,
    home: path === bot.cwd,
    recent: at >= listed.length,
    cwd: path === defaultFolder(bot) ? undefined : path,
  }))
}

/** Whether a new thread with `bot` working in `path` may name its place: `path` is one of its projects in a git repository. */
export const offersPlace = (bot: { projects: string[]; projectGitRepos: boolean[] }, path: string) =>
  bot.projectGitRepos[bot.projects.indexOf(path)] === true

/** The place a new thread shows until the user picks one, as the bot's worktree mode gives it; null for side by side,
 * where it depends on the threads working then (the host decides). */
export const modePlace = (mode: WorktreeMode | null): ThreadPlace | null =>
  mode === 'always' ? 'worktree' : mode === 'off' ? 'here' : null

/** What `startThread` is sent: `setup` with its place only where one is offered, and for a thread without a bot
 * (`plain`) the folder picked, else the home folder an older host expects; none where the host chooses the folder
 * (`hostChooses`: it makes the thread's own folder, or a workflow's thread works in the workflow's). */
export function sentSetup(setup: ThreadSetup, { offered, plain, home, hostChooses }: { offered: boolean; plain: boolean; home: string; hostChooses: boolean }): ThreadSetup {
  const { place, ...rest } = setup
  const cwd = plain && !hostChooses ? (rest.cwd ?? home) : rest.cwd
  return { ...rest, ...(cwd === undefined ? {} : { cwd }), ...(offered && place ? { place } : {}) }
}
