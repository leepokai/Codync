import { computerAccessStep, computerPermissionSettings, type ComputerPermission, type ScreenAgentState } from '@shared/computer-access'
import { normalizeScreen, type ScreenState } from '@shared/models'
import { Observable } from '../lib/observable'
import type { AppModel } from './app-model'

/** Read-only checks are safe on launch. Requests happen only after an explicit setup action. */
export class ComputerAccess extends Observable {
  screen: ScreenState | null = null
  agent: ScreenAgentState | null = null
  busy: ComputerPermission | 'enable' | null = null
  private checkError: string | null = null
  private actionError: string | null = null
  private lastRequest: ComputerPermission | null = null
  requested = new Set<ComputerPermission>()
  private refreshing = false
  private app: AppModel

  constructor(app: AppModel) { super(); this.app = app }

  get step() {
    return computerAccessStep(window.codync.platform, this.app.local?.connection.kind === 'online', this.screen, this.agent)
  }

  get helperName() { return this.screen?.permissionApp || 'Codync Screen' }
  get error() { return this.checkError ?? this.actionError }

  async refresh() {
    if (this.refreshing) return
    this.refreshing = true
    try {
      const local = this.app.local
      this.agent = await window.codync.app.screenAgentState()
      if (local?.client && local.connection.kind === 'online') {
        this.screen = normalizeScreen(await local.client.call<ScreenState>('screenStatus', {}))
      } else this.screen = null
      this.checkError = null
      const granted = this.lastRequest === 'portal'
        ? this.screen?.capture && this.screen.input
        : this.lastRequest && this.screen?.[this.lastRequest]
      if (granted || this.step === 'ready') this.actionError = null
    } catch (error) {
      this.screen = null
      this.checkError = message(error)
    } finally {
      this.refreshing = false
      this.changed()
    }
  }

  async enable() {
    if (this.busy) return
    this.busy = 'enable'
    this.actionError = null
    this.changed()
    try {
      await this.app.setRemoteScreen(true)
      if (this.app.screenError) throw new Error(this.app.screenError)
      await this.refresh()
    } catch (error) { this.actionError = message(error) }
    finally { this.busy = null; this.changed() }
  }

  async request(permission: ComputerPermission) {
    const client = this.app.local?.client
    if (!client || this.busy) return
    this.busy = permission
    this.actionError = null
    this.lastRequest = permission
    this.requested.add(permission)
    this.changed()
    try {
      await client.call('requestScreenPermission', { permission }, 125_000)
      await this.refresh()
    } catch (error) {
      this.actionError = message(error)
    } finally { this.busy = null; this.changed() }
  }

  openSettings(permission: keyof typeof computerPermissionSettings) {
    window.codync.app.openSettings(computerPermissionSettings[permission])
  }
}

function message(error: unknown): string {
  if (error instanceof Error) return error.message
  if (error && typeof error === 'object' && 'message' in error) return String(error.message)
  return String(error)
}
