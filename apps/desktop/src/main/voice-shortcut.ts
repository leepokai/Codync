import { spawn } from 'node:child_process'
import { createInterface } from 'node:readline'
import { app, ipcMain, type WebContents } from 'electron'
import { CommandTaps, DEFAULT_VOICE_SHORTCUT, VOICE_SHORTCUTS, type VoiceKey, type VoiceShortcut } from '../shared/voice-shortcut'
import { prefs } from './host-controller'
import { speechAvailable } from './speech'

// Every modifier change in any app, from an NSEvent global monitor, with the seconds since the
// last key press or click. Neither needs the Accessibility or Input Monitoring grant (other keys
// would), so the app learns only that a modifier moved, never what was typed. JXA reaches AppKit
// without a helper binary.
const MONITOR = `ObjC.import('AppKit'); ObjC.import('CoreGraphics')
const out = $.NSFileHandle.fileHandleWithStandardOutput
const since = (type) => $.CGEventSourceSecondsSinceLastEventType($.kCGEventSourceStateCombinedSessionState, type)
$.NSApplication.sharedApplication.setActivationPolicy($.NSApplicationActivationPolicyProhibited)
$.NSEvent.addGlobalMonitorForEventsMatchingMaskHandler($.NSEventMaskFlagsChanged, (e) => {
  const sinceInput = Math.min(since($.kCGEventKeyDown), since($.kCGEventLeftMouseDown), since($.kCGEventRightMouseDown), since($.kCGEventOtherMouseDown))
  out.writeData($(e.keyCode + ' ' + e.modifierFlags + ' ' + e.timestamp + ' ' + sinceInput + '\\n').dataUsingEncoding($.NSUTF8StringEncoding))
})
$.NSApp.run`

/** Reports the shortcut key's taps from any app until the returned stop. */
function watchTaps(shortcut: Exclude<VoiceShortcut, 'off'>, onKey: (key: VoiceKey) => void): () => void {
  const taps = new CommandTaps(shortcut)
  const proc = spawn('/usr/bin/osascript', ['-l', 'JavaScript', '-e', MONITOR], { stdio: ['ignore', 'pipe', 'pipe'] })
  createInterface({ input: proc.stdout }).on('line', (line) => {
    const [key = 0, flags = 0, at = 0, sinceInput = 0] = line.split(' ').map(Number)
    const tap = taps.feed({ key, flags, at, sinceInput })
    if (tap) onKey(tap)
  })
  proc.stderr.on('data', (chunk: Buffer) => console.error(`voice shortcut: ${chunk.toString('utf8').trim()}`))
  proc.on('error', (error) => console.error(`voice shortcut: couldn't watch the keyboard: ${error.message}`))
  let stopping = false
  // ponytail: logged only, not restarted; the setting or a relaunch starts it again.
  proc.on('exit', (code, signal) => {
    if (!stopping) console.error(`voice shortcut: the keyboard watcher exited (${code ?? signal})`)
  })
  return () => {
    stopping = true
    proc.kill()
  }
}

/**
 * The voice input shortcut (macOS, with on-device speech): watches the chosen ⌘ key while it's on
 * and hands its taps to `onKey`. The setting belongs to the desktop installation (`voiceShortcut`
 * in `host-prefs.json`).
 */
export class VoiceShortcutKeys {
  private stop: (() => void) | null = null
  /** The window with a call on: the call has the microphone, so taps do nothing meanwhile. */
  private call: WebContents | null = null

  constructor(private onKey: (key: VoiceKey) => void) {}

  get shortcut(): VoiceShortcut {
    if (!speechAvailable()) return 'off'
    const saved = prefs.get('voiceShortcut')
    return VOICE_SHORTCUTS.find((s) => s.id === saved)?.id ?? DEFAULT_VOICE_SHORTCUT
  }

  register() {
    ipcMain.handle('voice:shortcut', () => this.shortcut)
    ipcMain.handle('voice:setShortcut', (_e, shortcut: VoiceShortcut) => {
      prefs.set('voiceShortcut', shortcut)
      this.sync()
      return this.shortcut
    })
    ipcMain.on('voice:calling', (e, on: boolean) => (this.call = on ? e.sender : null))
    app.on('will-quit', () => this.stop?.())
    this.sync()
  }

  private sync() {
    this.stop?.()
    this.stop = null
    const shortcut = this.shortcut
    if (shortcut === 'off') return
    this.stop = watchTaps(shortcut, (key) => {
      if (!this.call || this.call.isDestroyed()) this.onKey(key)
    })
  }
}
