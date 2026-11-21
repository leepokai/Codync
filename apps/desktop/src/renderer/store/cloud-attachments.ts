import { useEffect } from 'react'
import type { Computer } from '@shared/models'
import { ChannelTransport } from '../client/host-client'
import { relayComputers } from '../lib/relay-plan'
import { account } from './account'
import type { AppModel } from './app-model'
import { BotStore } from './bot-store'
import type { CloudComputer } from './cloud'

/**
 * The account's other computers, used from this app like the iPhone does: each one approved for this device is
 * attached as a store over the relay's encrypted channel (its bots join the sidebar) and detached when it leaves
 * the account or turns this device away. A Codync Cloud computer is asked for access once per launch (it approves
 * this account's devices by itself); any other is asked from Settings (`AccountComputers`). Channels stay open only
 * while someone looks at the window, so a cloud computer can sleep. Call once, from ChatWindow.
 */
export function useCloudAttachments(app: AppModel) {
  useEffect(() => {
    const attached = new Map<string, BotStore>()
    // Each attached store's own changes: its connection reaches neither `app` nor `app.cloud`.
    const watches = new Map<string, () => void>()
    const asked = new Set<string>()
    let attended = false
    let offline = new Set<string>()

    const ask = (c: CloudComputer) => {
      if (asked.has(c.computerId)) return
      asked.add(c.computerId)
      void window.codync.cloud.requestAccess(c).then((res) => {
        if (!res.ok) console.warn('Codync Cloud access:', res.error.message ?? res.error.code)
        void app.cloud.refresh(true)
      })
    }

    const attach = (c: CloudComputer) => {
      const cached = app.computers.find((x) => x.id === c.computerId)
      const computer: Computer = {
        id: c.computerId,
        name: c.name,
        signKey: c.signKey,
        boxKey: c.boxKey ?? null,
        urls: [],
        cloud: account.cloudURL,
        device: c.device ?? cached?.device ?? null,
        color: cached?.color ?? null,
      }
      const store = new BotStore(computer, false, () => new ChannelTransport(c.computerId, c.signKey))
      store.setCloudHosting(c.cloud ?? null)
      if (!attended) store.pause()
      app.attach(computer, store)
      attached.set(c.computerId, store)
      watches.get(c.computerId)?.()
      watches.set(c.computerId, store.subscribe(noteConnections))
    }

    // Gone offline: the account's list says whether it's asleep (it wakes when this app connects) or off. Turned
    // away (its grant went): the list no longer has it as granted, so its store goes and Settings offers to ask again.
    const noteConnections = () => {
      const now = new Set([...attached].filter(([, s]) => s.connection.kind === 'computerOffline' || s.connection.kind === 'unauthorized').map(([id]) => id))
      if ([...now].some((id) => !offline.has(id))) void app.cloud.refresh(true)
      offline = now
    }

    const sync = () => {
      for (const c of app.cloud.computers) if (c.cloud && c.access === 'none') ask(c)
      const owned = (id: string) => !app.store(id) || app.store(id) === attached.get(id)
      const live = new Map(relayComputers(app.cloud.computers, app.local?.computer.id ?? null, owned).map((c) => [c.computerId, c]))
      for (const [id, c] of live) {
        const store = attached.get(id)
        if (store && store.computer.signKey === c.signKey && app.store(id) === store) store.setCloudHosting(c.cloud ?? null)
        else attach(c)
      }
      for (const [id, store] of attached) {
        if (live.has(id)) continue
        attached.delete(id)
        watches.get(id)?.()
        watches.delete(id)
        if (app.store(id) === store) app.detach(id)
      }
      noteConnections()
    }

    const setAttended = (on: boolean) => {
      if (on === attended) return
      attended = on
      for (const store of attached.values()) {
        if (on) store.resume()
        else store.pause()
      }
      if (on) void app.cloud.refresh(true)
    }

    sync()
    const offCloud = app.cloud.subscribe(sync)
    const offApp = app.subscribe(sync)
    const offAttended = window.codync.app.onAttended(setAttended)
    void window.codync.app.attended().then(setAttended)
    return () => {
      offCloud()
      offApp()
      offAttended()
      for (const off of watches.values()) off()
      for (const [id, store] of attached) if (app.store(id) === store) app.detach(id)
    }
  }, [app])
}
