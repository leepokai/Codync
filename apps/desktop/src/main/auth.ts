import { BrowserAuthentication } from './browser-authentication'
import { app, ipcMain, shell } from 'electron'
import { identity } from './environment'

// Browser sign-ins that come back to this app on a custom scheme (ASWebAuthenticationSession's
// job on the iPhone): connector OAuth (`codync://oauth`) and the account (`com.pokai.Codync://callback`).

export const SCHEMES = [identity.scheme, identity.appId]

const browser = new BrowserAuthentication((url) => shell.openExternal(url))

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
export const handleURL = (url: string) => browser.handleURL(url)

export function urlFromArgv(argv: string[]) {
  return argv.find((a) => SCHEMES.some((s) => a.toLowerCase().startsWith(`${s.toLowerCase()}:`)))
}

export const authenticate = (url: string, scheme: string, accept?: (url: string, isCurrent: () => boolean) => Promise<boolean>) =>
  browser.authenticate(url, scheme, accept)

export function registerAuthIPC() {
  ipcMain.handle('app:authenticate', (_e, url: string, scheme: string) => authenticate(url, scheme))
}
