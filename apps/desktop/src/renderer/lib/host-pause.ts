import { isBelow } from '../../shared/compat.ts'

/**
 * The first host release that can pause a bot even when hello omits `botPause`.
 *
 * 2.55.0, not the version in development. `release/2.54.0` carries `version = "2.54.0"` and no `paused` in
 * the bot config, so a host reporting exactly that, and not saying `botPause`, cannot honour the field and
 * would silently ignore it. A host that does honour it says `botPause: true` and keeps its real version, so
 * the updater does not treat a 2.54.0 build as the 2.55.0 release.
 */
export const PAUSE_HOST = '2.55.0'

/** Whether a host of `version` can pause a bot when it does not say otherwise. An unreadable version never is below. */
export const hostHasPause = (version: string): boolean => !isBelow(version, PAUSE_HOST)

/**
 * Whether the row menu and the editor offer Pause.
 * True when hello says `botPause`, or when the version is at least {@link PAUSE_HOST}. Missing and false are
 * the same: an older host omits the flag.
 */
export function offersBotPause(version: string | null, botPause?: boolean): boolean {
  if (botPause === true) return true
  return version !== null && hostHasPause(version)
}
