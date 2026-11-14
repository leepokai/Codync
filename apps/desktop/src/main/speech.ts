import { execFileSync, spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { existsSync } from 'node:fs'
import { join } from 'node:path'
import { app, ipcMain, type WebContents } from 'electron'
import type { SpeechEvent } from '../shared/ipc'

// On-device speech recognition for calls: a small Swift helper (native/speech-macos) runs
// SFSpeechRecognizer on the microphone and prints JSON lines. macOS only.

function helperPath() {
  const candidates = [
    app.isPackaged ? join(process.resourcesPath, 'codync-speech') : null,
    join(__dirname, '../../native/speech-macos/.build/release/codync-speech'),
    join(__dirname, '../../native/speech-macos/.build/debug/codync-speech'),
  ]
  return candidates.find((c): c is string => !!c && existsSync(c)) ?? null
}

/** Whether this build can transcribe on device. */
export const speechAvailable = () => process.platform === 'darwin' && helperPath() !== null

let child: ChildProcessWithoutNullStreams | null = null
let listener: WebContents | null = null
/** Bumped per start: a stopping process may still deliver its final result, an older one may not. */
let generation = 0

function emit(event: SpeechEvent) {
  if (listener && !listener.isDestroyed()) listener.send('speech:event', event)
}

function start(sender: WebContents, locale: string | null) {
  stop()
  // Another window's dictation or call loses the helper, and its exit (an older generation) won't say so.
  if (listener && listener !== sender) emit({ type: 'stopped' })
  listener = sender
  const path = helperPath()
  if (!path) return emit({ type: 'error', message: "On-device speech isn't available in this build." })
  const proc = spawn(path, locale ? ['--locale', locale] : [], { stdio: 'pipe' })
  const mine = ++generation
  child = proc
  let buffer = ''
  proc.stdout.on('data', (chunk: Buffer) => {
    buffer += chunk.toString('utf8')
    let nl: number
    while ((nl = buffer.indexOf('\n')) >= 0) {
      const line = buffer.slice(0, nl)
      buffer = buffer.slice(nl + 1)
      try {
        if (generation === mine) emit(JSON.parse(line) as SpeechEvent)
      } catch {}
    }
  })
  proc.on('exit', () => {
    if (child === proc) child = null
    if (generation === mine) emit({ type: 'stopped' })
  })
  proc.on('error', (error) => emit({ type: 'error', message: error.message }))
}

function stop() {
  const proc = child
  child = null
  if (proc && !proc.killed) proc.kill('SIGTERM')
}

let supported: string[] | null = null

/** The languages the recognizer can transcribe (identifiers like `zh-TW`), asked once. */
function locales() {
  if (supported) return supported
  const path = helperPath()
  try {
    supported = path ? (JSON.parse(execFileSync(path, ['--locales'], { encoding: 'utf8', timeout: 5000 })) as string[]) : []
  } catch {
    supported = []
  }
  return supported
}

export function registerSpeech() {
  ipcMain.on('speech:available', (e) => (e.returnValue = speechAvailable()))
  ipcMain.on('speech:locales', (e) => (e.returnValue = locales()))
  ipcMain.on('speech:start', (e, locale: string | null) => start(e.sender, locale))
  // Only the window using the helper stops it: one that lost it to another window has nothing to stop.
  ipcMain.on('speech:stop', (e) => {
    if (e.sender === listener) stop()
  })
  app.on('before-quit', stop)
}
