import { isBelow } from '../../shared/compat.ts'

/**
 * The first host release that can pause a bot: it answers nothing and spends nothing until it is resumed.
 *
 * 2.55.0, not the version in development. `release/2.54.0` (#589) carries `version = "2.54.0"` and no `paused` in
 * the bot config, so a host reporting exactly that cannot honour the field and would silently ignore it. A constant
 * that is too early shows a control the host ignores; one that is too late only hides a control that would have
 * worked. When in doubt, be late — and move this down only once a release that contains the field is cut.
 */
export const PAUSE_HOST = '2.55.0'

/** Whether a host of `version` can pause a bot. */
export const hostHasPause = (version: string): boolean => !isBelow(version, PAUSE_HOST)
