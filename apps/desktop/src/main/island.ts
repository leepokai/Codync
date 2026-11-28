import { execFile } from 'node:child_process'
import { BrowserWindow, globalShortcut, ipcMain, screen } from 'electron'
import { CONNECT_PATIENCE_MS, type CallResult, type WindowCommand } from '../shared/ipc'
import type { VoiceKey } from '../shared/voice-shortcut'
import { hitRect, hits, notchOf, PANEL, type IslandRect, type IslandScreen, type IslandSummary, type NotchScreen } from '../shared/island'
import { headless, present, webPreferences } from './headless'
import { holdChannel } from './channel'
import { prefs } from './host-controller'

// Every NSScreen's frame, safe-area top and the menu bar on each side of the notch. JXA reaches
// AppKit without a helper binary; it runs when the island opens and when displays change.
const SCREENS = `ObjC.import('AppKit'); JSON.stringify($.NSScreen.screens.js.map((s) => {
  const top = s.safeAreaInsets.top
  return { x: s.frame.origin.x, width: s.frame.size.width, top,
    left: top > 0 ? s.auxiliaryTopLeftArea.size.width : 0, right: top > 0 ? s.auxiliaryTopRightArea.size.width : 0 }
}))`

/** Linux on Wayland: a window can't place itself at the top of the screen or stay above others. */
export const islandSupported = process.platform !== 'linux' || process.env.XDG_SESSION_TYPE !== 'wayland'

/**
 * Linux (X11) only learns where the pointer is from its own windows' events, so it can't poll it
 * like macOS and Windows do: the window is shaped to the island instead, and the renderer sees
 * the pointer arrive.
 */
const shapedWindow = process.platform === 'linux'
/** A shrinking shape keeps its old outline until the close animation (0.34 s) is over. */
const SHRINK_MS = 400

function readScreens(): Promise<NotchScreen[]> {
  if (process.platform !== 'darwin') return Promise.resolve([])
  return new Promise((resolve) => {
    execFile('/usr/bin/osascript', ['-l', 'JavaScript', '-e', SCREENS], { timeout: 5000 }, (error, stdout) => {
      try {
        if (error) throw error
        resolve(JSON.parse(stdout) as NotchScreen[])
      } catch (e) {
        // Without the notch's size the island sits at the top edge like on a screen without one.
        console.error(`island: couldn't read the notch: ${e instanceof Error ? e.message : String(e)}`)
        resolve([])
      }
    })
  })
}

/** Answers the approval on screen from any app while it shows. */
const KEYS = { 'Alt+Y': 'allow', 'Alt+N': 'deny' } as const
/** A call the chat window never answers (it reloaded meanwhile) fails after the reconnect it may wait out, the call's own timeout, and this grace. */
const ANSWER_GRACE_MS = 5000
const noChatWindow: CallResult = { ok: false, error: { status: 0, message: 'Open the Codync window to dictate here.' } }

interface Deps {
  url: string
  preload: string
  /** Returns whether a chat window took the command. */
  send: (command: WindowCommand, show?: boolean) => boolean
  openApp: () => void
  /** The setting changed (the tray's Settings menu shows it). */
  changed: () => void
}

/**
 * The notch island's window: a fixed transparent panel at the top center of
 * the notch's display. The renderer animates the shape inside it and reports where it is; the
 * window takes the mouse only over that shape and lets every other click through.
 */
export class Island {
  private window: BrowserWindow | null = null
  private summary: IslandSummary = { bots: [], permissions: [], usage: [] }
  private screen: IslandScreen = { notch: null }
  private rect: IslandRect = { x: 0, y: 0, width: 0, height: 0 }
  private inside = false
  private timer: NodeJS.Timeout | null = null
  private shaped: IslandRect = { x: 0, y: 0, width: 0, height: 0 }
  private unfocus: NodeJS.Timeout | undefined
  /** Host calls the island made, waiting for the chat window's answer. */
  private calls = new Map<string, (result: CallResult) => void>()
  private callId = 0

  constructor(private deps: Deps) {}

  /** Off until turned on: the island is still experimental. */
  get enabled() {
    return islandSupported && prefs.get('showIsland') === true
  }

  setEnabled(on: boolean) {
    prefs.set('showIsland', on)
    this.sync()
    this.deps.changed()
    return this.enabled
  }

  register() {
    ipcMain.on('island:supported', (e) => (e.returnValue = islandSupported))
    ipcMain.handle('island:enabled', () => this.enabled)
    ipcMain.handle('island:setEnabled', (_e, on: boolean) => this.setEnabled(on))
    ipcMain.handle('island:state', () => ({ screen: this.screen, summary: this.summary }))
    ipcMain.on('island:summary', (_e, summary: IslandSummary) => {
      this.summary = summary
      this.window?.webContents.send('island:summary', summary)
    })
    ipcMain.on('island:shape', (_e, rect: IslandRect) => {
      this.rect = rect
      if (shapedWindow) this.shape()
    })
    ipcMain.on('island:respond', (_e, ref: string, entryId: string, optionId: string) =>
      this.deps.send({ kind: 'respondPermission', ref, entryId, optionId }, false),
    )
    ipcMain.on('island:openBot', (_e, ref: string) => this.deps.send({ kind: 'openBot', ref }))
    ipcMain.on('island:send', (_e, ref: string, text: string) => this.deps.send({ kind: 'sendMessage', ref, text }, false))
    ipcMain.on('island:newBot', () => this.deps.send({ kind: 'newBot' }))
    ipcMain.on('island:openApp', () => this.deps.openApp())
    ipcMain.on('island:openVoiceSettings', () => this.deps.send({ kind: 'voiceSettings' }))
    ipcMain.handle('island:call', (_e, ref: string, method: string, body: unknown, timeoutMs: number) => this.call(ref, method, body, timeoutMs))
    ipcMain.on('island:answer', (_e, id: string, result: CallResult) => this.calls.get(id)?.(result))
    ipcMain.on('island:focusable', (_e, on: boolean) => this.setFocusable(on))
    ipcMain.on('island:keys', (_e, on: boolean) => this.setKeys(on))
    screen.on('display-added', () => void this.place())
    screen.on('display-removed', () => void this.place())
    screen.on('display-metrics-changed', () => void this.place())
    this.sync()
  }

  /** Hands the island's host call to the chat window and waits for its answer (`island:answer`). */
  private call(ref: string, method: string, body: unknown, timeoutMs: number) {
    const id = String(++this.callId)
    // The island's own window has no connection: the chat window makes the call, so its computer's
    // channel stays open for it even while nobody looks at the chat window.
    const at = ref.indexOf('/')
    const release = holdChannel(at < 0 ? ref : ref.slice(0, at))
    return new Promise<CallResult>((resolve) => {
      const timer = setTimeout(() => settle(noChatWindow), CONNECT_PATIENCE_MS + timeoutMs + ANSWER_GRACE_MS)
      const settle = (result: CallResult) => {
        clearTimeout(timer)
        this.calls.delete(id)
        release()
        resolve(result)
      }
      this.calls.set(id, settle)
      if (!this.deps.send({ kind: 'islandCall', id, ref, method, body, timeoutMs }, false)) settle(noChatWindow)
    })
  }

  private sync() {
    if (this.enabled) this.open()
    else this.close()
  }

  private open() {
    if (this.window) return
    const isMac = process.platform === 'darwin'
    // Top center of the main display until the notch is measured (place() moves it if needed).
    const main = screen.getPrimaryDisplay().bounds
    const win = new BrowserWindow({
      ...PANEL,
      x: Math.round(main.x + (main.width - PANEL.width) / 2),
      y: main.y,
      show: false,
      frame: false,
      transparent: true,
      backgroundColor: '#00000000',
      hasShadow: false,
      resizable: false,
      movable: false,
      minimizable: false,
      maximizable: false,
      fullscreenable: false,
      focusable: false,
      skipTaskbar: true,
      // Lets the panel cover the menu bar.
      enableLargerThanScreen: true,
      acceptFirstMouse: true,
      // macOS: a non-activating panel on every Space and over full-screen apps; Windows: out of Alt+Tab.
      ...(isMac ? { type: 'panel' } : process.platform === 'win32' ? { type: 'toolbar' } : {}),
      webPreferences: webPreferences(this.deps.preload),
    })
    // Above the menu bar on macOS (main menu + 3).
    win.setAlwaysOnTop(true, isMac ? 'main-menu' : 'screen-saver', isMac ? 3 : 0)
    if (!isMac) win.setVisibleOnAllWorkspaces(true)
    // Takes no mouse until the renderer reports the shape.
    this.rect = { x: 0, y: 0, width: 0, height: 0 }
    this.shaped = this.rect
    if (shapedWindow) win.setShape([this.shaped])
    else win.setIgnoreMouseEvents(true)
    win.once('ready-to-show', () => present(win, true))
    win.on('closed', () => {
      if (this.window === win) this.window = null
    })
    this.window = win
    this.inside = false
    void this.place()
    void win.loadURL(this.deps.url)
    if (!shapedWindow) this.tick()
  }

  private close() {
    this.setKeys(false)
    clearTimeout(this.unfocus)
    if (this.timer) clearTimeout(this.timer)
    this.timer = null
    this.window?.destroy()
    this.window = null
  }

  /** Only the message field takes the keyboard; a panel takes it without bringing the app forward. */
  private setFocusable(on: boolean) {
    const win = this.window
    if (!win) return
    clearTimeout(this.unfocus)
    win.setFocusable(on)
    if (on) win.focus()
    else if (win.isFocused()) {
      // Giving the keyboard back takes the window off screen for a moment on macOS (and to the back
      // of its level): wait until the shape has folded, then bring it forward again.
      this.unfocus = setTimeout(() => {
        if (!win.isDestroyed() && win.isFocused()) {
          win.blur()
          win.moveTop()
        }
      }, SHRINK_MS)
    }
  }

  private setKeys(on: boolean) {
    for (const [accelerator, answer] of Object.entries(KEYS)) {
      // A headless run's island is never seen: keys typed in another app mustn't answer an approval there.
      if (!on || headless) {
        globalShortcut.unregister(accelerator)
        continue
      }
      if (globalShortcut.isRegistered(accelerator)) continue
      if (!globalShortcut.register(accelerator, () => this.window?.webContents.send('island:key', answer))) {
        console.error(`island: ${accelerator} is taken by another app`)
      }
    }
  }

  /** The voice input shortcut's taps, while the island is on. */
  voiceKey(key: VoiceKey) {
    this.window?.webContents.send('island:voiceKey', key)
  }

  /** Top center of the display with the notch, else the main display. */
  private async place() {
    if (!this.window) return
    const screens = await readScreens()
    const win = this.window
    if (!win || win.isDestroyed()) return
    const display = screen.getAllDisplays().find((d) => notchOf(screens, d.bounds)) ?? screen.getPrimaryDisplay()
    this.screen = { notch: notchOf(screens, display.bounds) }
    const b = display.bounds
    win.setBounds({ x: Math.round(b.x + (b.width - PANEL.width) / 2), y: b.y, ...PANEL })
    win.webContents.send('island:screen', this.screen)
  }

  /** Linux: the window draws and takes the mouse only over the island. */
  private shape() {
    const win = this.window
    if (!win) return
    const next = hitRect(this.rect)
    const grows = next.width * next.height >= this.shaped.width * this.shaped.height
    if (this.timer) clearTimeout(this.timer)
    const apply = () => {
      this.shaped = next
      if (!win.isDestroyed()) win.setShape([next])
    }
    if (grows) apply()
    else this.timer = setTimeout(apply, SHRINK_MS)
  }

  /** macOS and Windows: follows the pointer, 60 Hz near the panel and a few times a second elsewhere. */
  private tick = () => {
    const win = this.window
    if (!win || win.isDestroyed()) return
    const point = screen.getCursorScreenPoint()
    const bounds = win.getBounds()
    const x = point.x - bounds.x
    const y = point.y - bounds.y
    const inside = hits(this.rect, x, y)
    if (inside !== this.inside) {
      this.inside = inside
      win.setIgnoreMouseEvents(!inside)
      win.webContents.send('island:hover', inside)
    }
    const near = x > -60 && x < bounds.width + 60 && y < bounds.height + 60
    this.timer = setTimeout(this.tick, near ? 16 : 150)
  }
}
