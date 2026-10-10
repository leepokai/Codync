import { contextBridge, ipcRenderer, type IpcRendererEvent } from 'electron'
import { sshBridge } from './ssh'
import type { CallError, CodyncBridge, HostSnapshot, TraySummary, WindowCommand } from '../shared/ipc'

let streamId = 0
const streams = new Map<string, { onData: (line: string) => void; onEnd: (error: CallError | null) => void }>()

ipcRenderer.on('host:stream:data', (_e, id: string, line: string) => streams.get(id)?.onData(line))
ipcRenderer.on('host:stream:end', (_e, id: string, error: CallError | null) => {
  const s = streams.get(id)
  streams.delete(id)
  s?.onEnd(error)
})

function listen<T>(channel: string, cb: (value: T) => void) {
  const handler = (_e: IpcRendererEvent, value: T) => cb(value)
  ipcRenderer.on(channel, handler)
  return () => void ipcRenderer.removeListener(channel, handler)
}

const bridge: CodyncBridge = {
  platform: process.platform as CodyncBridge['platform'],
  appName: ipcRenderer.sendSync('app:name') as string,
  appScheme: ipcRenderer.sendSync('app:scheme') as string,
  hostPort: ipcRenderer.sendSync('app:hostPort') as number,
  appVersion: ipcRenderer.sendSync('app:version') as string,
  computerName: ipcRenderer.sendSync('app:computerName') as string,
  debugOpen: ipcRenderer.sendSync('app:debugOpen') as string | null,
  host: {
    snapshot: () => ipcRenderer.invoke('host:snapshot') as Promise<HostSnapshot>,
    onChange: (cb) => listen('host:change', cb),
    install: () => ipcRenderer.send('host:install'),
    restart: () => ipcRenderer.send('host:restart'),
    uninstall: () => ipcRenderer.send('host:uninstall'),
    openLog: () => ipcRenderer.send('host:openLog'),
    async call(baseURL, token, method, body, timeoutMs) {
      const result = (await ipcRenderer.invoke('host:call', baseURL, token, method, body, timeoutMs)) as
        | { ok: true; value: unknown }
        | { ok: false; error: CallError }
      if (!result.ok) throw result.error
      return result.value
    },
    stream(url, token, onData, onEnd) {
      const id = `s${++streamId}`
      streams.set(id, { onData, onEnd })
      ipcRenderer.send('host:stream:open', id, url, token)
      return () => {
        if (!streams.delete(id)) return
        ipcRenderer.send('host:stream:close', id)
      }
    },
    health: (baseURL) => ipcRenderer.invoke('host:health', baseURL),
  },
  account: {
    state: () => ipcRenderer.invoke('account:state'),
    onChange: (cb) => listen('account:change', cb),
    signIn: (provider) => ipcRenderer.invoke('account:signIn', provider),
    signOut: () => ipcRenderer.invoke('account:signOut'),
    token: () => ipcRenderer.invoke('account:token'),
  },
  speech: {
    available: ipcRenderer.sendSync('speech:available') as boolean,
    locales: () => ipcRenderer.sendSync('speech:locales') as string[],
    start: (locale) => ipcRenderer.send('speech:start', locale),
    stop: () => ipcRenderer.send('speech:stop'),
    onEvent: (cb) => listen('speech:event', cb),
  },
  ssh: sshBridge,
  cloud: {
    request: (method, path, body, signed) => ipcRenderer.invoke('cloud:request', method, path, body, signed),
  },
  updates: {
    state: () => ipcRenderer.invoke('updates:state'),
    onChange: (cb) => listen('updates:change', cb),
    check: () => ipcRenderer.send('updates:check'),
    setAutoCheck: (on) => ipcRenderer.send('updates:autoCheck', on),
    setAutoDownload: (on) => ipcRenderer.send('updates:autoDownload', on),
  },
  files: {
    begin: (file) => ipcRenderer.invoke('files:begin', file),
    write: (id, offset, data) => ipcRenderer.invoke('files:write', id, offset, data),
    finish: (id) => ipcRenderer.invoke('files:finish', id),
    cancel: (id) => ipcRenderer.invoke('files:cancel', id),
  },
  app: {
    openExternal: (url) => ipcRenderer.send('app:openExternal', url),
    copy: (text) => ipcRenderer.send('app:copy', text),
    pickFiles: () => ipcRenderer.invoke('app:pickFiles'),
    readClipboardFiles: () => ipcRenderer.invoke('app:clipboardFiles'),
    setTraySummary: (summary: TraySummary) => ipcRenderer.send('app:traySummary', summary),
    onCommand: (cb: (c: WindowCommand) => void) => listen('app:command', cb),
    openPairing: () => ipcRenderer.send('app:openPairing'),
    quit: () => ipcRenderer.send('app:quit'),
    resetAllData: () => ipcRenderer.invoke('app:resetAllData'),
    setLaunchAtLogin: (on) => ipcRenderer.invoke('app:setLaunchAtLogin', on),
    showInDock: () => ipcRenderer.invoke('app:showInDock'),
    setShowInDock: (on) => ipcRenderer.invoke('app:setShowInDock', on),
    launchAtLogin: () => ipcRenderer.invoke('app:launchAtLogin'),
    openSettings: (url) => ipcRenderer.send('app:openSettings', url),
    authenticate: (url, scheme) => ipcRenderer.invoke('app:authenticate', url, scheme),
    screenAgentState: () => ipcRenderer.invoke('screen:state'),
    setScreenAgent: (on) => ipcRenderer.invoke('screen:setAgent', on),
    syncScreenAgent: (enabled) => ipcRenderer.invoke('screen:syncAgent', enabled),
  },
}

contextBridge.exposeInMainWorld('codync', bridge)
