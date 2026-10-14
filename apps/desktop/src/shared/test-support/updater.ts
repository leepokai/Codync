import { EventEmitter } from 'node:events'
import path from 'node:path'
import * as compat from '../compat.ts'
import type { UpdateState } from '../ipc.ts'
import { loadMainModule, settle } from './main-module.ts'

interface UpdatesInstance extends EventEmitter {
  state: UpdateState
  register(windows: () => unknown[]): void
  check(): void
  setAutoCheck(on: boolean): void
  setAutoDownload(on: boolean): void
}

type ShipIt = { request: boolean; job: 'none' | 'idle' | 'running' }

export function updaterFixture(options: { packaged?: boolean; main?: boolean; prefs?: Record<string, unknown>; shipIt?: ShipIt } = {}) {
  const calls = { check: 0, download: 0, install: 0, prepare: 0, resume: 0, quit: 0, fetch: 0 }
  // macOS's installer job: whether its update request exists, and its launchd state.
  const shipIt = { ...(options.shipIt ?? { request: true, job: 'idle' }), removed: 0 }
  const launchctl = (_bin: string, args: string[], done: (error: Error | null, stdout: string) => void) => {
    if (args[0] === 'remove') {
      shipIt.removed++
      return done(null, '')
    }
    if (shipIt.job === 'none') return done(new Error('Could not find service'), '')
    done(null, shipIt.job === 'running' ? '{\n\t"PID" = 42;\n};' : '{\n\t"LastExitStatus" = 256;\n};')
  }
  const opened: string[] = []
  const preferences = new Map(Object.entries(options.prefs ?? {}))
  const timers = new Map<number, { callback: () => unknown; ms: number }>()
  let nextTimer = 0
  let now = Date.UTC(2026, 9, 9)
  const behavior = {
    prepare: async () => {},
    check: async () => {},
    download: async () => {},
    install: () => {},
    hostFactsFail: false,
    minApp: '2.0.0',
    devices: [{ platform: 'ios' }],
    storeVersion: '2.0.0' as string | null,
    focused: false,
    idleSeconds: 600,
    hostIdle: true,
  }
  const app = Object.assign(new EventEmitter(), {
    isPackaged: options.packaged ?? true,
    quit: () => { calls.quit++ },
    getPath: () => '/Applications/Codync.app/Contents/MacOS/Codync',
  })
  const handlers = new Map<string, (...args: unknown[]) => unknown>()
  const ipc = Object.assign(new EventEmitter(), {
    handle: (name: string, fn: (...args: unknown[]) => unknown) => handlers.set(name, fn),
  })
  const updater = Object.assign(new EventEmitter(), {
    autoDownload: true,
    autoInstallOnAppQuit: false,
    checkForUpdates: async () => { calls.check++; updater.emit('checking-for-update'); await behavior.check() },
    downloadUpdate: async () => { calls.download++; await behavior.download() },
    quitAndInstall: () => { calls.install++; behavior.install() },
  })
  const host = {
    prepareForUpdate: async () => { calls.prepare++; await behavior.prepare() },
    resumeAfterCancelledUpdate: () => { calls.resume++ },
    isIdleForUpdate: async () => behavior.hostIdle,
    call: async (method: string) => {
      if (behavior.hostFactsFail) throw new Error('Host unreachable')
      return method === 'hello' ? { minApp: behavior.minApp } : { devices: behavior.devices }
    },
  }
  const timer = (callback: () => unknown, ms: number) => {
    const id = ++nextTimer
    timers.set(id, { callback, ms })
    return id
  }
  const { Updates } = loadMainModule<{ Updates: new (host: unknown) => UpdatesInstance }>('updates', {
    'node:child_process': { execFile: launchctl },
    'node:events': { EventEmitter },
    'node:fs': { existsSync: (file: string) => shipIt.request && file === '/Users/me/Library/Caches/com.pokai.Codync.ShipIt/ShipItState.plist' },
    'node:os': { homedir: () => '/Users/me' },
    'node:path': path,
    electron: { app, ipcMain: ipc, shell: { openPath: async (target: string) => { opened.push(target); return '' } }, BrowserWindow: { getFocusedWindow: () => behavior.focused }, powerMonitor: { getSystemIdleTime: () => behavior.idleSeconds } },
    'electron-updater': { autoUpdater: updater },
    '../shared/compat': compat,
    './account': { isMainEnvironment: () => options.main ?? true },
    './environment': { identity: { appId: 'com.pokai.Codync' } },
    './host-controller': { prefs: { get: (key: string) => preferences.get(key), set: (key: string, value: unknown) => preferences.set(key, value) } },
  }, {
    Date: class extends Date { static override now() { return now } },
    process: { platform: 'darwin' },
    setTimeout: timer, setInterval: timer, clearInterval: (id: number) => timers.delete(id),
    fetch: async () => {
      calls.fetch++
      if (behavior.storeVersion === null) throw new Error('Offline')
      return { json: async () => ({ results: [{ version: behavior.storeVersion }] }) }
    },
  })
  const updates = new Updates(host)
  const sent: { window: number; channel: string; state: UpdateState }[] = []
  updates.register(() => [0, 1].map((window) => ({ webContents: { send: (channel: string, state: UpdateState) => sent.push({ window, channel, state: { ...state } }) } })))
  const emit = async (event: string, value?: unknown) => { updater.emit(event, value); await settle() }
  const tick = async (ms: number) => {
    for (const [id, entry] of [...timers]) {
      if (entry.ms === ms && timers.has(id)) await entry.callback()
    }
    await settle()
  }
  return { updates, app, ipc, handlers, updater, calls, opened, behavior, preferences, sent, timers, shipIt, emit, tick, advance: (ms: number) => { now += ms } }
}
