import type { ScreenState } from './models'

export interface ScreenAgentState {
  available: boolean
  needsApproval: boolean
  registered: boolean
}

export type ComputerPermission = 'capture' | 'input' | 'portal'
export type ComputerAccessStep = 'host' | 'unavailable' | 'enable' | 'background' | 'helper' | ComputerPermission | 'ready'

/** First registration is explicit; only a previously enabled helper resumes after an app update. */
export function shouldSyncScreenAgent(enabled: boolean, status: string, resumeAfterUpdate: boolean): boolean {
  return enabled && (status === 'enabled' || (status === 'not-registered' && resumeAfterUpdate))
}

/** OS-specific requirements, based only on observed state; opening Settings never grants access. */
export function computerAccessStep(platform: string, online: boolean, screen: ScreenState | null, agent: ScreenAgentState | null): ComputerAccessStep {
  if (!online || !screen) return 'host'
  if (platform === 'win32') return screen.computerUse ? 'ready' : 'host'
  if (platform === 'darwin' && agent?.available === false) return 'unavailable'
  if (!screen.enabled) return 'enable'
  if (platform === 'darwin' && agent?.needsApproval) return 'background'
  if (platform === 'darwin' && agent?.registered === false) return 'enable'
  if (!screen.connected) return 'helper'
  if (platform === 'linux') return screen.capture && screen.input ? 'ready' : 'portal'
  if (!screen.capture) return 'capture'
  if (!screen.input) return 'input'
  return 'ready'
}

export const computerPermissionSettings = {
  capture: 'x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture',
  input: 'x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility',
  background: 'x-apple.systempreferences:com.apple.LoginItems-Settings.extension',
} as const
