// `npm run dev` in a linked worktree runs a sandbox beside the main checkout's Codync Dev: its own
// host data (`~/.codync-dev-<name>`), port, app data and host process (tools/dev.mjs).

export const SANDBOX_PREFIX = '.codync-dev-'

/** A worktree folder name made safe for a data folder and a window title. */
export const sandboxName = (folder: string) => folder.replace(/[^A-Za-z0-9._-]+/g, '-')

/** Ports for sandboxes, lowest first, without the ones other sandboxes hold; 19222/19223 stay with Codync and Codync Dev. */
export function sandboxPorts(claimed: Iterable<number>): number[] {
  const taken = new Set(claimed)
  return Array.from({ length: 100 }, (_, i) => 19300 + i).filter((port) => !taken.has(port))
}

/** The Clerk development secret key a sandbox may use: the environment's, else the `.env` file's `CODYNC_CLERK_SECRET_KEY`; null when neither is set. Anything but an `sk_test_` key throws, so production is never reached. */
export function devClerkSecretKey(fromEnv: string | undefined, envFile: string | undefined) {
  const raw = fromEnv ?? /^CODYNC_CLERK_SECRET_KEY=(.+)$/m.exec(envFile ?? '')?.[1] ?? ''
  const key = raw.trim().replace(/^["']|["']$/g, '')
  if (key && !key.startsWith('sk_test_')) throw new Error('CODYNC_CLERK_SECRET_KEY must be the development instance key (sk_test_...).')
  return key || null
}

/** `Codync Dev (feat-x)` for `~/.codync-dev-feat-x`; any other host data folder shows by its name. */
export function sandboxLabel(name: string, home: string) {
  const folder = home.split(/[\\/]/).filter(Boolean).pop() ?? home
  return `${name} (${folder.startsWith(SANDBOX_PREFIX) ? folder.slice(SANDBOX_PREFIX.length) : folder})`
}
