import type { ComputerAccessStep } from '../../shared/computer-access'
import type { ScreenState } from '../../shared/models'

export type AccessPage = 'helper' | 'capture' | 'input' | 'portal' | 'control' | 'star'

export function accessPageReady(page: AccessPage, step: ComputerAccessStep, screen: ScreenState | null): boolean {
  if (!screen || step === 'host' || step === 'unavailable') return false
  switch (page) {
    case 'helper': return screen.enabled && screen.connected && !['enable', 'helper', 'background'].includes(step)
    case 'capture': return screen.capture
    case 'input': return screen.input
    case 'portal': return screen.capture && screen.input
    case 'control': return step === 'ready'
    case 'star': return false
  }
}

/** Only list permissions that can actually be granted on this desktop. */
export function accessPages(platform: string): { id: AccessPage; label: string }[] {
  if (platform === 'win32') return [{ id: 'control', label: 'Computer control' }, { id: 'star', label: 'GitHub Star' }]
  return [
    { id: 'helper', label: 'Codync Screen' },
    ...(platform === 'darwin'
      ? [{ id: 'capture' as const, label: 'Screen recording' }, { id: 'input' as const, label: 'Control apps' }]
      : [{ id: 'portal' as const, label: 'Screen sharing & control' }]),
    { id: 'star', label: 'GitHub Star' },
  ]
}
