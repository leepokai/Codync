import { app, ipcMain, shell } from 'electron'
import { identity } from './environment'

// Browser sign-ins that come back to this app on a custom scheme (ASWebAuthenticationSession's
// job on the native apps): connector OAuth (`codync://oauth`) and the account (`com.pokai.Codync://callback`).

export const SCHEMES = [identity.scheme, identity.appId]

interface Pending {
  scheme: string
  resolve: (url: string) => void
  reject: (error: Error) => void
  timer: NodeJS.Timeout
}

let pending: Pending | null = null

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
  const scheme = url.split(':')[0]?.toLowerCase()
  if (pending && scheme === pending.scheme.toLowerCase()) {
    clearTimeout(pending.timer)
    pending.resolve(url)
    pending = null
  }
}

export function urlFromArgv(argv: string[]) {
  return argv.find((a) => SCHEMES.some((s) => a.toLowerCase().startsWith(`${s.toLowerCase()}:`)))
}

/** Opens `url` in the browser and waits (up to 10 minutes) for the return on `scheme`. */
export function authenticate(url: string, scheme: string): Promise<string> {
  if (!/^https:\/\//.test(url)) return Promise.reject(new Error('Sign-in needs an https address.'))
  pending?.reject(new Error('Another sign-in started.'))
  return new Promise((resolve, reject) => {
    pending = {
      scheme,
      resolve,
      reject,
      timer: setTimeout(() => {
        pending = null
        reject(new Error('Sign-in timed out.'))
      }, 10 * 60_000),
    }
    void shell.openExternal(url)
  })
}

export function registerAuthIPC() {
  ipcMain.handle('app:authenticate', (_e, url: string, scheme: string) => authenticate(url, scheme))
}
