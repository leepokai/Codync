import { ipcMain, BrowserWindow } from 'electron'
import { join } from 'node:path'
import { isTunnelVncPort } from './ssh'
import type { ScreenView, ScreenWindowTarget } from '../shared/screen-viewer'
import { bridgeVnc } from './vnc-bridge'

interface Live {
  vncPort: number
  view: ScreenView
  close: () => void
  window: BrowserWindow
}

const live = new Map<string, Live>()

/** One window per computer. The picture travels through the SSH tunnel, then this loopback bridge. */
export function registerScreenViewer(rendererURL: (page: string) => string) {
  ipcMain.handle('screen:open', async (_event, target: ScreenWindowTarget) => {
    if (!target?.computerId || !isTunnelVncPort(target.vncPort)) {
      throw new Error("This computer's screen tunnel isn't open. Connect Over SSH and try again.")
    }
    const current = live.get(target.computerId)
    if (current && !current.window.isDestroyed() && current.vncPort === target.vncPort) {
      current.window.show()
      current.window.focus()
      return
    }
    if (current) {
      current.close()
      if (!current.window.isDestroyed()) current.window.destroy()
      live.delete(target.computerId)
    }
    const bridge = await bridgeVnc(target.vncPort)
    const window = new BrowserWindow({
      width: 1280,
      height: 800,
      minWidth: 640,
      minHeight: 400,
      title: target.name ? `Screen · ${target.name}` : 'Screen',
      backgroundColor: '#000000',
      autoHideMenuBar: true,
      webPreferences: {
        preload: join(__dirname, '../preload/index.js'),
        sandbox: true,
        contextIsolation: true,
        backgroundThrottling: false,
      },
    })
    const entry: Live = {
      vncPort: target.vncPort,
      view: { computerId: target.computerId, name: target.name, url: bridge.url },
      close: bridge.close,
      window,
    }
    live.set(target.computerId, entry)
    // Keys must reach the remote computer, not the Edit menu.
    window.webContents.setIgnoreMenuShortcuts(true)
    window.on('closed', () => {
      if (live.get(target.computerId)?.window !== window) return
      bridge.close()
      live.delete(target.computerId)
    })
    void window.loadURL(`${rendererURL('screen')}&computer=${encodeURIComponent(target.computerId)}`)
  })

  ipcMain.handle('screen:target', (_event, computerId: string) => live.get(computerId)?.view ?? null)
}
