import type { Bot } from '../../../shared/models.ts'

/** Matches --working-duration in thread.css. */
export const workingExitMs = 420

/** Preserve the last live bot through exit, until its animation has finished. */
export function workingTransition(current: Bot, previous: Bot, visible: boolean, mounted: boolean) {
  const active = visible ? current : previous
  return { active, indicator: mounted ? active : null }
}
