import type { ScreenState } from '@shared/models'
import type { Failure } from './screen-recovery'
import type { Phase } from './screen-session'

/**
 * Why the screen can't be viewed yet, and what to do on that computer (the phones'
 * `accessGuidance`): only the computer itself can grant OS permissions. `tr` takes literals.
 */
export function accessGuidance(screen: ScreenState, tr: (text: string) => string): string {
  if (screen.platform === 'windows') return tr('Remote screen viewing is not available on Windows yet. Bots can still use apps on that computer.')
  const setup = tr('On the computer, open Codync → Settings → Computer access. Permissions are set separately on each computer.')
  const app = screen.permissionApp || 'Codync Screen'
  if (!screen.enabled) return tr('Remote screen is off. {setup}').replaceAll('{setup}', setup)
  if (!screen.connected) return tr('Codync Screen is not running. {setup}').replaceAll('{setup}', setup)
  if (screen.platform === 'linux' && (!screen.capture || !screen.input)) return tr('Choose a screen and allow control in the system sharing dialog. {setup}').replaceAll('{setup}', setup)
  if (!screen.capture) return tr('Allow screen recording for {name}. {setup}').replaceAll('{name}', app).replaceAll('{setup}', setup)
  if (!screen.input) return tr('Allow computer control for {name}. macOS may call this Device Control and Data Access or Accessibility. {setup}').replaceAll('{name}', app).replaceAll('{setup}', setup)
  return tr('Computer access is ready.')
}

/** What a failed viewer says: this app's wording translated, the host's own text as it wrote it. `tr` takes literals. */
export function failureText(failure: Failure, tr: (text: string) => string): string {
  switch (failure.reason) {
    case 'connect': return tr("Couldn't connect to the remote screen. Check the computer's connection and try again.")
    case 'unreachable': return tr("Couldn't reach the computer.")
    case 'host': return failure.detail ?? tr("Couldn't reconnect to the remote screen.")
    case 'reconnect': return tr("Couldn't reconnect to the remote screen.")
  }
}

/** What covers a viewer in place of the video: an icon or a spinner, a line, and whether a retry goes with it. */
export interface Notice {
  icon?: string
  text: string
  retry?: boolean
  /** A small note over the video, not a cover. */
  dim?: boolean
}

/** The notice a viewer (window or preview) shows for its state, or null when the video plays. `tr` takes literals. */
export function screenNotice(
  { live, unreachable, screen, phase }: { live: boolean; unreachable: boolean; screen: ScreenState | null; phase: Phase | undefined },
  tr: (text: string) => string,
): Notice | null {
  if (!live) {
    if (unreachable && !screen) return { icon: 'wifi.slash', text: tr('This computer is offline.') }
    if (!screen) return { text: tr('Connecting to the screen…') }
    return { icon: 'lock.display', text: accessGuidance(screen, tr) }
  }
  switch (phase?.kind) {
    case 'connecting': return { text: tr('Connecting to the screen…') }
    case 'reconnecting': return { text: tr('Reconnecting…'), dim: true }
    case 'failed': return { icon: 'exclamationmark.triangle', text: failureText(phase.failure, tr), retry: true }
    default: return null
  }
}
