import type { ScreenState } from '@shared/models'

/** Whether a computer's screen can be viewed now: Remote screen on, the helper running, recording allowed (the phones' `available`). */
export function screenAvailable(screen: ScreenState | null): boolean {
  return !!screen && screen.enabled && screen.connected && screen.capture
}

/** A computer's "Remote screen" button: another computer (not this one), reachable, its screen available. */
export function offersRemoteScreen(screen: ScreenState | null, online: boolean, isThisComputer: boolean): boolean {
  return !isThisComputer && online && screenAvailable(screen)
}
