import type { ScreenDisplay, ScreenState } from '@shared/models'
import { fill } from './fill.ts'

/** The screen can be watched: on, its helper running, and allowed to record. */
export const screenAvailable = (s: ScreenState | null) => !!s && s.enabled && s.connected && s.capture

export const mainDisplay = (s: ScreenState): ScreenDisplay | undefined => s.displays.find((d) => d.main) ?? s.displays[0]

const SETUP = 'On the computer, open Codync → Settings → Computer access. Permissions are set separately on each computer.'

/** What keeps a computer's screen from this app, and where to fix it (the iPhone's `accessGuidance`). */
export function screenGuidance(s: ScreenState, tr: (text: string) => string): string {
  if (s.platform === 'windows') return tr('Remote screen viewing is not available on Windows yet. Bots can still use apps on that computer.')
  const setup = tr(SETUP)
  const name = s.permissionApp ?? 'Codync Screen'
  if (!s.enabled) return fill(tr('Remote screen is off. {setup}'), { setup })
  if (!s.connected) return fill(tr('Codync Screen is not running. {setup}'), { setup })
  if (!s.capture || (s.platform === 'linux' && !s.input)) {
    if (s.platform === 'linux') return fill(tr('Choose a screen and allow control in the system sharing dialog. {setup}'), { setup })
    return fill(tr('Allow screen recording for {name}. {setup}'), { name, setup })
  }
  if (!s.input) return fill(tr('Allow computer control for {name}. macOS may call this Device Control and Data Access or Accessibility. {setup}'), { name, setup })
  return tr('Computer access is ready.')
}
