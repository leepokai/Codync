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
