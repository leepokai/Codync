import { normalizeScreen, type ScreenDisplay, type ScreenState } from '@shared/models'
import { Observable } from '../../lib/observable'
import { screenAvailable } from './screen-access'
import { ScreenControl } from './screen-control'
import { closeViewerSession, ScreenInput } from './screen-input'
import { ScreenLink } from './screen-link'
import { openingMode, type ViewerMode } from './screen-mode'
import { messageOf } from './screen-recovery'
import { ScreenSession } from './screen-session'
import { NO_INSETS, ScreenViewport, type Insets, type Size } from './screen-viewport'

const POLL_MS = 3_000

export interface ScreenViewerOptions {
  /** Never takes control and sends no input: the preview in the details panel. */
  viewOnly?: boolean
  /** Space kept clear around the video. */
  insets?: Insets
}

/** The Remote screen window's state for one computer: its status, the live session, who is in control, and input. */
export class ScreenViewerModel extends Observable {
  name = ''
  screen: ScreenState | null = null
  /** The last status call failed: the computer can't be reached. */
  unreachable = false
  session: ScreenSession | null = null
  private readonly viewOnly: boolean
  private readonly insets: Insets
  readonly input: ScreenInput
  private link: ScreenLink
  private readonly control: ScreenControl
  private selected: number | null = null
  private viewSize: Size = { width: 0, height: 0 }
  private pixelRatio = 1
  private timer: ReturnType<typeof setInterval> | null = null
  private stopped = false
  private refreshing = false
  private wasLive = false

  constructor(computerId: string, { viewOnly = false, insets = NO_INSETS }: ScreenViewerOptions = {}) {
    super()
    this.viewOnly = viewOnly
    this.insets = insets
    this.link = new ScreenLink(computerId)
    this.control = new ScreenControl({
      canControl: () => this.screen?.input === true && this.session !== null,
      // Not while reconnecting: a takeover then outlives a recovery that fails and closes the old id.
      hasSession: () => this.session?.phase.kind === 'live',
      takeover: (on) => this.link.callNow('screenTakeover', { on }).then(() => {}),
      update: () => {
        this.input.interactive = this.control.interactive
        this.changed()
      },
      refresh: () => void this.refresh(),
      fail: (error) => console.info(`screen takeover failed: ${messageOf(error) ?? error}`),
    })
    this.input = new ScreenInput((message) => this.session?.send(message), new ScreenViewport({ width: 0, height: 0 }, { width: 0, height: 0 }, insets))
    // Nothing is sent until a mode allows it.
    this.input.interactive = false
  }

  /** This window controls the computer (bots may still look). */
  get interactive(): boolean {
    return this.control.interactive
  }

  get mode(): ViewerMode {
    return this.interactive ? 'control' : 'view'
  }

  /** The display on show: the chosen one, else the computer's main. */
  get display(): ScreenDisplay | null {
    const displays = this.screen?.displays ?? []
    return displays.find((d) => d.id === this.selected) ?? displays.find((d) => d.main) ?? displays[0] ?? null
  }

  start() {
    if (!this.viewOnly) {
      void this.link.call<{ name?: string }>('hello').then((hello) => {
        this.name = hello.name ?? ''
        this.changed()
      }, () => {})
    }
    void this.refresh()
    this.timer = setInterval(() => void this.refresh(), POLL_MS)
  }

  /** Closes everything: the viewer on the computer, control handed back, the poll stopped. Safe to call twice. */
  stop() {
    if (this.stopped) return
    this.stopped = true
    if (this.timer) clearInterval(this.timer)
    this.timer = null
    this.endSession()
  }

  async refresh() {
    if (this.refreshing || this.stopped) return
    this.refreshing = true
    try {
      this.screen = normalizeScreen(await this.link.call<ScreenState>('screenStatus'))
      this.unreachable = false
    } catch (error) {
      this.unreachable = true
      console.info(`screen status failed: ${messageOf(error) ?? error}`)
    } finally {
      this.refreshing = false
    }
    if (this.stopped) return
    if (this.screen && !this.screen.input && this.interactive) this.setMode('view')
    if (!screenAvailable(this.screen)) this.endSession()
    else if (!this.session) this.beginSession()
    else if (this.screen) this.control.sync(this.screen.userControl)
    this.syncViewport()
    this.changed()
  }

  private beginSession() {
    const display = this.display
    if (!display) return
    const session = new ScreenSession(this.link, display)
    session.onRelease = () => this.input.releaseAll()
    // A session that was replaced reports late: only the current one (or the end of the last) reaches the main process.
    session.onSession = (id) => { if (this.session === session || (id === null && this.session === null)) this.reportSession(id) }
    this.session = session
    this.wasLive = false
    session.subscribe(() => this.sessionChanged(session))
    session.start()
    // A window looks while a bot works on the screen and drives otherwise; the choice then outlives reconnects.
    this.control.preferred ??= this.viewOnly ? 'view' : openingMode(this.screen)
    this.control.set(this.control.preferred === 'control')
    this.syncViewport()
  }

  private endSession() {
    if (!closeViewerSession(this, this.input)) return
    this.control.set(false)
    this.changed()
  }

  /** A failed session closed on the computer, which gave control back to the bots. */
  private sessionChanged(session: ScreenSession) {
    if (session === this.session && session.phase.kind === 'failed') this.control.sessionFailed()
    // Control the host dropped while the connection was lost is asked for again as soon as the video is back.
    const live = session.phase.kind === 'live'
    if (live && !this.wasLive && session === this.session) {
      this.control.resume()
      void this.refresh()
    }
    this.wasLive = live
    this.changed()
  }

  retry() {
    this.session?.start()
    this.control.set(this.control.preferred === 'control')
  }

  /** The window's View / Take control switch. A preview ignores it. */
  setMode(mode: ViewerMode) {
    if (this.viewOnly) return
    this.control.setMode(mode)
  }

  switchDisplay(display: ScreenDisplay) {
    this.selected = display.id
    this.session?.switchDisplay(display)
    this.syncViewport()
    this.changed()
  }

  /** The video box (CSS points) and the device pixel ratio changed. */
  setView(size: Size, devicePixelRatio: number) {
    if (size.width === this.viewSize.width && size.height === this.viewSize.height && devicePixelRatio === this.pixelRatio) return
    this.viewSize = size
    this.pixelRatio = devicePixelRatio
    this.syncViewport()
    this.changed()
  }

  private syncViewport() {
    const display = this.display
    if (!display) return
    const viewport = new ScreenViewport({ width: display.width, height: display.height }, this.viewSize, this.insets)
    this.input.viewport = viewport
    if (viewport.area.width > 0 && viewport.area.height > 0) this.session?.setPixels(viewport.pixels(this.pixelRatio))
  }

  private reportSession(id: string | null) {
    const target = this.link.target
    // Only a Remote screen window reports: the main process closes its session if the window dies.
    if (target && !this.viewOnly) window.codync.screenViewer.report(target.baseURL, target.token, id)
  }

  // MARK: clipboard

  /** Cmd+V: types what this computer's clipboard holds into the remote one. */
  async paste() {
    const text = await window.codync.app.readClipboardText()
    if (text) this.input.text(text)
  }

  async sendClipboard() {
    const text = await window.codync.app.readClipboardText()
    if (text && this.interactive) this.session?.pushClipboard(text)
  }

  copyFromComputer() {
    const text = this.session?.takeRemoteClipboard()
    if (text) window.codync.app.copy(text)
  }
}
