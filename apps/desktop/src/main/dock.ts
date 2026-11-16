import { app } from 'electron'
import { headless } from './headless'
import { prefs } from './host-controller'

export const showInDock = () => prefs.get('showInDock') !== false

export function setShowInDock(on: boolean) {
  prefs.set('showInDock', on)
  applyDockVisibility()
  return showInDock()
}

export function applyDockVisibility() {
  if (process.platform !== 'darwin') return
  if (showInDock() && !headless) void app.dock?.show()
  else app.dock?.hide()
}
