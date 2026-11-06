import { useEffect } from 'react'
import type { Computer } from '@shared/models'
import type { SSHConfigHosts, SSHState } from '@shared/ssh'
import { LoopbackTransport } from '../../client/host-client'
import { Observable, useModel } from '../../lib/observable'
import type { AppModel } from '../../store/app-model'
import { BotStore } from '../../store/bot-store'

/** The main process's SSH computers, mirrored for views. */
class SSHModel extends Observable {
  state: SSHState = { profiles: [], status: {}, attachments: [] }
  /** `~/.ssh/config`'s hosts and whether the user allowed reading it; null until known. */
  config: SSHConfigHosts | null = null
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

  /** Re-reads the hosts on each visit to Computers, so ones added to the config since show up. */
  async loadConfig() {
    this.applyConfig(await window.codync.ssh.configHosts())
  }

  async answerConfig(access: 'allowed' | 'denied') {
    this.applyConfig(await window.codync.ssh.setConfigAccess(access))
  }

  private applyConfig(c: SSHConfigHosts) {
    this.config = c
    this.changed()
  }

  /** Config hosts that aren't SSH computers yet. */
  get newConfigHosts() {
    const added = new Set(this.state.profiles.map((p) => p.host))
    return (this.config?.hosts ?? []).filter((h) => !added.has(h.alias))
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
