import { Observable, useModel } from '../../lib/observable'
import type { BotStore } from '../../store/bot-store'
import { market, needsSignIn, sleep, type InstalledConnector, type InstalledSkill, type MarketApp, type MarketConnector } from './market-models'
import { signInCode } from './sign-in-code'

/** What's installed on one computer, for the Marketplace and bot settings (kit BotStore's plugin state). */
export class Plugins extends Observable {
  installedConnectors: InstalledConnector[] = []
  installedSkills: InstalledSkill[] = []

  constructor(readonly store: BotStore) {
    super()
  }

  async refresh() {
    const client = this.store.client
    if (!client) return
    const [c, s] = await Promise.allSettled([market.connectors(client), market.skills(client)])
    if (c.status === 'fulfilled') this.installedConnectors = c.value
    if (s.status === 'fulfilled') this.installedSkills = s.value
    this.changed()
  }

  private async after<T>(op: Promise<T>) {
    const result = await op
    await this.refresh()
    return result
  }

  async installConnector(item: MarketConnector, option: string, inputs: Record<string, string>) {
    return this.after(market.installConnector(await this.store.ready(), item.name, option, inputs))
  }

  async addConnector(body: Parameters<typeof market.addConnector>[1]) {
    return this.after(market.addConnector(await this.store.ready(), body))
  }

  async importConnectors(config: string) {
    return this.after(market.importConnectors(await this.store.ready(), config))
  }

  async finishConnectorSignIn(state: string, code: string | null, error: string | null, iss: string | null = null) {
    return this.after(market.finishConnectorSignIn(await this.store.ready(), state, code, error, iss))
  }

  /**
   * Waits while the user signs in in the computer's browser (the host catches the return).
   * A connector with a saved token reads `none` until then, so only `signedIn` ends the wait.
   */
  async waitForSignIn(id: string) {
    for (let i = 0; i < 150; i++) {
      await sleep(2000)
      await this.refresh()
      if (this.installedConnectors.find((c) => c.id === id)?.auth === 'signedIn') return
    }
  }

  async removeConnector(id: string) {
    await this.after(market.removeConnector(await this.store.ready(), id))
  }

  async installSkill(source: string) {
    await this.after(market.installSkill(await this.store.ready(), source))
  }

  /** Adds a marketplace app with the values the user typed, and starts its sign-in. */
  async installApp(app: MarketApp, values: Record<string, string>) {
    const connector = await this.after(market.installMarketApp(await this.store.ready(), app.id, values))
    if (needsSignIn(connector)) await this.signIn(connector.id)
    return connector
  }

  async addSkill(name: string, description: string, instructions: string) {
    await this.after(market.addSkill(await this.store.ready(), name, description, instructions))
  }

  async removeSkill(id: string) {
    await this.after(market.removeSkill(await this.store.ready(), id))
  }

  /**
   * Signs in to a remote connector (host/src/market/oauth.rs). With an `app` callback the system
   * browser returns `codync://oauth?…` and the code goes to the host; with `host` the browser
   * returns to the host directly and we wait for the connector to flip to signed in.
   * A sign-in with a code (GitHub) shows the code first, then waits the same way.
   * False when the user cancels, so the caller stops there too.
   */
  async signIn(id: string) {
    const plan = await market.connectorSignIn(await this.store.ready(), id)
    if (!URL.canParse(plan.url)) return false
    if (plan.code) {
      if (!(await signInCode.ask(plan.code))) return false
      window.codync.app.copy(plan.code)
    }
    if (plan.callback !== 'app') {
      window.codync.app.openExternal(plan.url)
      await this.waitForSignIn(id)
      return true
    }
    let back: string
    try {
      back = await window.codync.app.authenticate(plan.url, window.codync.appScheme)
    } catch {
      return false
    }
    const items = new URL(back).searchParams
    await this.finishConnectorSignIn(items.get('state') ?? '', items.get('code'), items.get('error_description') ?? items.get('error'), items.get('iss'))
    return true
  }
}

const byStore = new WeakMap<BotStore, Plugins>()

export function pluginsFor(store: BotStore) {
  let p = byStore.get(store)
  if (!p) byStore.set(store, (p = new Plugins(store)))
  return p
}

/** The store's plugin state, re-rendering on its changes. */
export const usePlugins = (store: BotStore) => useModel(pluginsFor(store))
