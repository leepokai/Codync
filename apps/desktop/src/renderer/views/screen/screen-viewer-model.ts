import { normalizeScreen, type ScreenDisplay, type ScreenState } from '@shared/models'
import { Observable } from '../../lib/observable'
import { screenAvailable } from './screen-access'
import { ScreenInput } from './screen-input'
import { ScreenLink } from './screen-link'
import { messageOf } from './screen-recovery'
import { ScreenSession } from './screen-session'
import { ScreenViewport, type Size } from './screen-viewport'

const POLL_MS = 3_000

/** The Remote screen window's state for one computer: its status, the live session, who is in control, and input. */
export class ScreenViewerModel extends Observable {
  name = ''
  screen: ScreenState | null = null
  /** The last status call failed: the computer can't be reached. */
  unreachable = false
  session: ScreenSession | null = null
  /** This window controls the computer (bots may still look). */
  interactive = false
  readonly input: ScreenInput
  private link: ScreenLink
  private tookOver = false
  private selected: number | null = null
  private viewSize: Size = { width: 0, height: 0 }
  private pixelRatio = 1
  private timer: ReturnType<typeof setInterval> | null = null
  private stopped = false
  private refreshing = false

  constructor(computerId: string) {
    super()
    this.link = new ScreenLink(computerId)
    this.input = new ScreenInput((message) => this.session?.send(message), new ScreenViewport({ width: 0, height: 0 }, { width: 0, height: 0 }))
  }

  /** The display on show: the chosen one, else the computer's main. */
  get display(): ScreenDisplay | null {
    const displays = this.screen?.displays ?? []
    return displays.find((d) => d.id === this.selected) ?? displays.find((d) => d.main) ?? displays[0] ?? null
  }

  start() {
    void this.link.call<{ name?: string }>('hello').then((hello) => {
      this.name = hello.name ?? ''
      this.changed()
    }, () => {})
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
    if (this.screen && !this.screen.input && this.interactive) this.setInteractive(false)
    if (!screenAvailable(this.screen)) this.endSession()
    else if (!this.session) this.beginSession()
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
    session.subscribe(() => this.sessionChanged(session))
    session.start()
    // Opened from the computer's menu to use it: take the screen (bots may still look).
    this.setInteractive(true)
    this.syncViewport()
  }

  private endSession() {
    const session = this.session
    if (!session) return
    this.session = null
    this.input.releaseAll()
    session.close()
    this.setInteractive(false)
    this.changed()
  }

  /** A failed session closed on the computer, which gave control back to the bots. */
  private sessionChanged(session: ScreenSession) {
    if (session === this.session && session.phase.kind === 'failed' && this.tookOver) {
      this.tookOver = false
      this.interactive = false
      this.input.interactive = false
    }
    this.changed()
  }

  retry() {
    this.session?.start()
    this.setInteractive(true)
  }

  /** Control needs the computer's input permission; without it the window only looks. */
  setInteractive(on: boolean) {
    const allowed = on && this.screen?.input === true && this.session !== null
    this.interactive = allowed
    this.input.interactive = allowed
    if (allowed !== this.tookOver) {
      this.tookOver = allowed
      this.link.callNow('screenTakeover', { on: allowed }).then(() => this.refresh(), (error) => console.info(`screen takeover failed: ${messageOf(error) ?? error}`))
    }
    this.changed()
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
    const viewport = new ScreenViewport({ width: display.width, height: display.height }, this.viewSize)
    this.input.viewport = viewport
    if (viewport.view.width > 0) this.session?.setPixels(viewport.pixels(this.pixelRatio))
  }

  private reportSession(id: string | null) {
    const target = this.link.target
    if (target) window.codync.screenViewer.report(target.baseURL, target.token, id)
  }

  // MARK: clipboard

  /** Cmd+V: types what this computer's clipboard holds into the remote one. */
  async paste() {
    const text = await window.codync.app.readClipboardText()
    if (text) this.input.text(text)
  }

  async sendClipboard() {
    const text = await window.codync.app.readClipboardText()
    if (text) this.session?.pushClipboard(text)
  }

  copyFromComputer() {
    const text = this.session?.takeRemoteClipboard()
    if (text) window.codync.app.copy(text)
  }
}
