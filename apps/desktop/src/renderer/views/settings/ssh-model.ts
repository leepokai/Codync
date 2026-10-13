import { useEffect } from 'react'
import type { Computer } from '@shared/models'
import type { SSHState } from '@shared/ssh'
import { LoopbackTransport } from '../../client/host-client'
import { Observable, useModel } from '../../lib/observable'
import type { AppModel } from '../../store/app-model'
import { BotStore } from '../../store/bot-store'

/** The main process's SSH computers, mirrored for views. */
class SSHModel extends Observable {
  state: SSHState = { profiles: [], status: {}, attachments: [] }
  private started = false

  start() {
    if (this.started) return
    this.started = true
    window.codync.ssh.onChange((s) => this.apply(s))
    void window.codync.ssh.state().then((s) => this.apply(s))
  }

  private apply(s: SSHState) {
    this.state = s
    this.changed()
  }

  status(id: string) {
    return this.state.status[id] ?? { kind: 'idle' as const }
  }

  isSSH(computerId: string) {
    return this.state.attachments.some((a) => a.computer.id === computerId)
  }
}

export const ssh = new SSHModel()

export function useSSH() {
  useEffect(() => ssh.start(), [])
  return useModel(ssh)
}

/**
 * Attaches each connected SSH computer to the window's model as a loopback store, and detaches
 * it when its tunnel ends. Call once, from ChatWindow.
 */
export function useSSHAttachments(app: AppModel) {
  useEffect(() => {
    ssh.start()
    // computer id → the tunnel it's attached through
    const attached = new Map<string, string>()
    const sync = () => {
      const live = new Map(ssh.state.attachments.map((a) => [a.computer.id, a]))
      for (const [id, a] of live) {
        const key = `${a.baseURL}|${a.token}`
        if (attached.get(id) === key) continue
        const cached = app.computers.find((c) => c.id === id)
        const computer: Computer = { ...a.computer, urls: [], color: cached?.color ?? null, device: cached?.device ?? null }
        app.attach(computer, new BotStore(computer, true, () => new LoopbackTransport(a.baseURL, a.token)))
        attached.set(id, key)
      }
      for (const id of [...attached.keys()]) {
        if (live.has(id)) continue
        attached.delete(id)
        app.detach(id)
      }
    }
    sync()
    const off = ssh.subscribe(sync)
    return () => {
      off()
      for (const id of attached.keys()) app.detach(id)
    }
  }, [app])
}
