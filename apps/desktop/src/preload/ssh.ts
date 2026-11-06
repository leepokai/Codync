import { ipcRenderer, type IpcRendererEvent } from 'electron'
import type { SSHBridge, SSHConfigHosts, SSHState } from '../shared/ssh'

/** `window.codync.ssh`: SSH computers, run by the main process (main/ssh.ts). */
export const sshBridge: SSHBridge = {
  state: () => ipcRenderer.invoke('ssh:state') as Promise<SSHState>,
  onChange(cb) {
    const handler = (_e: IpcRendererEvent, s: SSHState) => cb(s)
    ipcRenderer.on('ssh:change', handler)
    return () => void ipcRenderer.removeListener('ssh:change', handler)
  },
  save: (profile) => ipcRenderer.invoke('ssh:save', profile) as Promise<string | null>,
  remove: (id) => ipcRenderer.send('ssh:remove', id),
  connect: (id) => ipcRenderer.send('ssh:connect', id),
  disconnect: (id) => ipcRenderer.send('ssh:disconnect', id),
  trustHostKey: (id) => ipcRenderer.send('ssh:trustHostKey', id),
  chooseKey: () => ipcRenderer.invoke('ssh:chooseKey') as Promise<string | null>,
  configHosts: () => ipcRenderer.invoke('ssh:configHosts') as Promise<SSHConfigHosts>,
  setConfigAccess: (access) => ipcRenderer.invoke('ssh:setConfigAccess', access) as Promise<SSHConfigHosts>,
}
