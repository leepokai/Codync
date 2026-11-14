import { ipcMain, nativeTheme } from 'electron'
import { appearance } from '../shared/appearance'
import { prefs } from './host-controller'

/** Electron applies this to every window's CSS, canvas media queries and native chrome. */
export function registerAppearance() {
  nativeTheme.themeSource = appearance(prefs.get('appearance'))
  ipcMain.handle('app:appearance', () => nativeTheme.themeSource)
  ipcMain.handle('app:setAppearance', (_event, value: unknown) => {
    const selected = appearance(value)
    prefs.set('appearance', selected)
    nativeTheme.themeSource = selected
    return selected
  })
}
