import { app } from 'electron'

// Open at login (macOS and Windows; Electron has no login item on Linux). Windows starts the
// login item with a flag so that launch can tell itself apart; macOS says so itself.

const LOGIN_FLAG = '--opened-at-login'
const options = process.platform === 'win32' ? { args: [LOGIN_FLAG] } : {}

export const launchAtLogin = () => app.getLoginItemSettings(options).openAtLogin

export function setLaunchAtLogin(on: boolean) {
  app.setLoginItemSettings({ ...options, openAtLogin: on })
  return launchAtLogin()
}

/** This launch came from the login item (`app_opened`'s `at_login`). */
export const openedAtLogin = () =>
  process.platform === 'darwin' ? app.getLoginItemSettings().wasOpenedAtLogin : process.argv.includes(LOGIN_FLAG)

/** Windows: a login item registered without the flag gets it, so its sign-in launches count. */
export function updateLoginItem() {
  if (process.platform !== 'win32' || launchAtLogin()) return
  // Quoted: Electron splits its own unquoted path at the first space ("Codync Dev.exe"), so the entry never matches.
  if (app.getLoginItemSettings({ path: `"${process.execPath}"` }).executableWillLaunchAtLogin) setLaunchAtLogin(true)
}
