import { app, ipcMain, Notification } from 'electron'
import type { DesktopAlert } from '../shared/alerts'
import { notificationFailure, type ErrorReport } from '../shared/error-report'
import type { WindowCommand } from '../shared/ipc'
import { identity } from './environment'
import { prefs } from './host-controller'

// Hosts' alerts as system notifications (shared/alerts.ts). Off by default: a phone paired to the
// same computer gets each one as a push, so turning both on alerts twice.

const PREF = 'systemNotifications'

/** Shown and not yet clicked or closed: a notification nothing holds is collected, and its click lost. */
const shown = new Set<Notification>()

const enabled = () => prefs.get(PREF) === true

function show(alert: DesktopAlert, send: (command: WindowCommand) => void, report: (report: ErrorReport) => void) {
  if (!enabled() || !Notification.isSupported()) return
  const notification = new Notification({ title: alert.title, body: alert.body })
  const forget = () => void shown.delete(notification)
  notification.on('click', () => {
    forget()
    send({ kind: 'openAlert', ref: alert.ref, alert: alert.kind, workflowId: alert.workflowId })
  })
  notification.on('close', forget)
  notification.on('failed', (_e, error) => {
    forget()
    console.error(`notifications: couldn't show one: ${error}`)
    // Electron's message stays here; the report carries only the step that failed and its result code.
    report({ process: 'desktop-main', type: 'NotificationFailed', mechanism: 'notification', value: notificationFailure(String(error)), frames: [] })
  })
  shown.add(notification)
  notification.show()
}

/** `send` shows the chat window and gives it a command; `report` sends a notification that failed to error tracking. */
export function registerNotifications(send: (command: WindowCommand) => void, report: (report: ErrorReport) => void) {
  // Windows ties a toast (its name, and the click back to the app) to the Start Menu shortcut's ID, the installer's appId.
  if (process.platform === 'win32') app.setAppUserModelId(identity.appId)
  ipcMain.on('notifications:supported', (e) => (e.returnValue = Notification.isSupported()))
  ipcMain.handle('notifications:enabled', () => enabled())
  ipcMain.handle('notifications:setEnabled', (_e, on: boolean) => {
    prefs.set(PREF, on)
    return enabled()
  })
  ipcMain.on('notifications:show', (_e, alert: DesktopAlert) => show(alert, send, report))
}
