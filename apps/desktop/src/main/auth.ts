import { BrowserAuthentication } from './browser-authentication'
import { app, BrowserWindow, ipcMain, shell } from 'electron'
import { identity } from './environment'
import { activate, present } from './headless'

// Browser sign-ins that come back to this app on a custom scheme (ASWebAuthenticationSession's
// job on the iPhone): connector OAuth (`codync://oauth`) and the account (`com.pokai.Codync://callback`).

export const SCHEMES = [identity.scheme, identity.appId]

const browser = new BrowserAuthentication((url) => shell.openExternal(url))
/** The window a sign-in started from, brought back in front when the browser returns. */
let origin: BrowserWindow | null = null

export { AuthenticationEnded } from './browser-authentication'

export function registerSchemes() {
  for (const scheme of SCHEMES) {
    if (process.defaultApp && process.argv[1]) app.setAsDefaultProtocolClient(scheme, process.execPath, [process.argv[1]])
    else app.setAsDefaultProtocolClient(scheme)
  }
  // macOS delivers the URL here; Linux starts a second instance with it in argv.
  app.on('open-url', (event, url) => {
    event.preventDefault()
    handleURL(url)
  })
}

/** A URL on one of our schemes reached the app (open-url, or a second instance's argv). */
export function handleURL(url: string) {
  if (!browser.handleURL(url) || !origin || origin.isDestroyed()) return
  activate()
  present(origin)
}

export function urlFromArgv(argv: string[]) {
  return argv.find((a) => SCHEMES.some((s) => a.toLowerCase().startsWith(`${s.toLowerCase()}:`)))
}

export function authenticate(url: string, scheme: string, accept?: (url: string, isCurrent: () => boolean) => Promise<boolean>, window: BrowserWindow | null = null) {
  origin = window
  return browser.authenticate(url, scheme, accept)
}

export function registerAuthIPC() {
  ipcMain.handle('app:authenticate', (e, url: string, scheme: string) => authenticate(url, scheme, undefined, BrowserWindow.fromWebContents(e.sender)))
}
