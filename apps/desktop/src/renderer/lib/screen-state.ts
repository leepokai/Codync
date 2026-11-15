import type { ScreenDisplay, ScreenState } from '@shared/models'
import type { ScreenPhase } from '../store/screen-session'
import { fill } from './fill.ts'

/** The screen can be watched: on, its helper running, and allowed to record. */
export const screenAvailable = (s: ScreenState | null) => !!s && s.enabled && s.connected && s.capture

/** Bots can use the computer: Windows says so itself; elsewhere the screen is available and control is allowed. */
export const controlReady = (s: ScreenState) => s.platform === 'windows' ? s.computerUse : screenAvailable(s) && s.input

export const mainDisplay = (s: ScreenState): ScreenDisplay | undefined => s.displays.find((d) => d.main) ?? s.displays[0]

/** The display a new session shows: the one picked before (a pause or an outage ended the last session) while the
 * computer still has it, else the main one. */
export const displayToShow = (s: ScreenState, picked: number | null): ScreenDisplay | undefined =>
  s.displays.find((d) => d.id === picked) ?? mainDisplay(s)

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

/** What shows instead of the picture: a spinner without `icon`, and a retry for a failed session. */
export interface ScreenNotice {
  icon?: string
  text: string
  retry?: () => void
}

/** The computer as the picture needs it: its screen, whether it's a cloud computer asleep (looking wakes it), and its
 * connection. */
export interface ScreenComputer {
  screen: ScreenState | null
  asleep: boolean
  connection: 'online' | 'connecting' | 'offline'
}

/** What shows instead of the picture, through `tr`; null once it's live. */
export function screenNotice(
  c: ScreenComputer,
  session: { phase: ScreenPhase; start(): Promise<void> } | null,
  available: boolean,
  tr: (text: string) => string,
): ScreenNotice | null {
  if (!available) {
    if (c.asleep) return { text: tr('Waking the computer…') }
    if (c.connection === 'connecting') return { text: tr('Connecting to the screen…') }
    if (c.connection === 'offline') return { icon: 'wifi.slash', text: tr('This computer is offline.') }
    if (!c.screen) return { icon: 'arrow.down.circle', text: tr('Update Codync on your computer to use its screen from here.') }
    return { icon: 'lock.display', text: screenGuidance(c.screen, tr) }
  }
  const phase = session?.phase
  if (!session || !phase || phase.kind === 'connecting') return { text: tr('Connecting to the screen…') }
  if (phase.kind === 'reconnecting') return { text: tr('Reconnecting…') }
  if (phase.kind === 'failed') return { icon: 'exclamationmark.triangle', text: tr(phase.message), retry: () => void session.start() }
  return null
}

/** How long a computer's screen streams with nobody touching the app before it pauses (it resumes on a click). */
export const SCREEN_IDLE_MS = 5 * 60_000

/** Whether a screen last touched at `touched` (ms) has gone untouched long enough by `now` to pause. */
export const untouchedTooLong = (touched: number, now: number) => now - touched >= SCREEN_IDLE_MS
