import { app, type BrowserWindow } from 'electron'

/**
 * `CODYNC_HEADLESS` (development builds only): an e2e sandbox keeps every window off the screen, so a test run never
 * takes the screen, the Dock or the keyboard from whoever is using the computer. The pages still render and Playwright
 * drives them over the DevTools protocol (docs/guides/test-stack.md#headless-by-default).
 */
export const headless = !app.isPackaged && Boolean(process.env.CODYNC_HEADLESS)

/** Every window's `webPreferences`: a page whose window is never shown must keep drawing and running its timers. */
export const webPreferences = (preload: string) => ({ preload, sandbox: true, contextIsolation: true, backgroundThrottling: !headless })

/** Shows `win`, taking the keyboard unless `inactive`. A headless run leaves it hidden. */
export function present(win: BrowserWindow, inactive = false) {
  if (headless) return
  if (inactive) return win.showInactive()
  win.show()
  win.focus()
}

/** macOS: brings the app in front of the others. A headless run stays in the background. */
export function activate() {
  if (!headless && process.platform === 'darwin') app.focus({ steal: true })
}
