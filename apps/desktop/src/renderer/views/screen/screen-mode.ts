import type { ScreenState } from '@shared/models'

/** What a window does with the mouse and keyboard: `view` sends nothing, `control` drives the computer. */
export type ViewerMode = 'view' | 'control'

/**
 * The mode a Remote screen window opens in: it looks while a bot is working on the screen (so as not to
 * fight it) and drives otherwise. A computer that doesn't allow control can only be looked at.
 */
export function openingMode(screen: ScreenState | null): ViewerMode {
  if (!screen?.input) return 'view'
  return screen.agentBot ? 'view' : 'control'
}

/** The live preview in the details panel: running, giving way to the full window, or not worth the bandwidth. */
export type ThumbnailPhase = 'running' | 'window' | 'idle'

/**
 * One viewer per computer is enough: while the full window is open the preview lets go of its session, and
 * while the app window is hidden or minimized nobody sees it. (The panel closing, its tab changing or the
 * bot changing unmount the preview, which ends it too.)
 */
export function thumbnailPhase({ windowOpen, hidden }: { windowOpen: boolean; hidden: boolean }): ThumbnailPhase {
  if (hidden) return 'idle'
  return windowOpen ? 'window' : 'running'
}
