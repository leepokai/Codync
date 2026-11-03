// Who drives the computer: the window's View / Take control choice and the host's `userControl` flag
// that backs it. The host drops that flag when its last viewer session closes (a reconnect that lost
// the connection), so the window asks again, and looks until the host has answered.
import type { ViewerMode } from './screen-mode.ts'

export interface ControlHost {
  /** The computer allows control and a session is open. */
  canControl(): boolean
  /** The session is live (not connecting or reconnecting): only then may control be asked for. */
  hasSession(): boolean
  /** `screenTakeover`. */
  takeover(on: boolean): Promise<void>
  /** `interactive` changed: the input follows it and the views re-render. */
  update(): void
  /** Ask the host for its status again. */
  refresh(): void
  fail(error: unknown): void
}

export class ScreenControl {
  /** The mode the user chose, kept across reconnects; null until the first session picks the opening one. */
  preferred: ViewerMode | null = null
  /** This window controls the computer (bots may still look). */
  interactive = false
  /** The host was asked to give this window control and hasn't been asked to take it back. */
  private held = false
  private pending = 0
  private readonly host: ControlHost

  constructor(host: ControlHost) {
    this.host = host
  }

  setMode(mode: ViewerMode) {
    this.preferred = mode
    this.set(mode === 'control')
  }

  /** Control needs the computer's input permission and a session; without them the window only looks. */
  set(on: boolean) {
    const allowed = on && this.host.canControl()
    this.interactive = allowed
    if (allowed !== this.held) {
      this.held = allowed
      this.send(allowed)
    }
    this.host.update()
  }

  /** The session failed: control goes back to the bots. The host keeps `userControl` while another viewer (a phone)
   * stays connected, so it is handed back explicitly before the takeover is forgotten. */
  sessionFailed() {
    if (!this.held) return
    this.held = false
    this.interactive = false
    this.host.takeover(false).catch(() => {})
    this.host.update()
  }

  /** The video came back: a chosen control that was lost (a refused takeover) is asked for again. */
  resume() {
    if (!this.held && this.preferred === 'control') this.set(true)
  }

  /** With each status from the host: control it dropped on its own is asked for again, and the window only looks until it is back. */
  sync(userControl: boolean) {
    if (!this.held || userControl || this.pending > 0 || !this.host.hasSession()) return
    this.interactive = false
    this.host.update()
    this.send(true)
  }

  private send(on: boolean) {
    this.pending++
    this.host.takeover(on).then(() => true, (error) => { this.host.fail(error); return false }).then((ok) => {
      this.pending--
      // Only the newest request decides: an older one finishing late must not undo a later choice.
      if (on && this.held && this.pending === 0) {
        if (ok) this.interactive = this.preferred === 'control' && this.host.canControl()
        else {
          // Control isn't ours and can't be had now: look. The choice stays; `resume` asks again once the video is back.
          this.held = false
          this.interactive = false
        }
        this.host.update()
      }
      this.host.refresh()
    })
  }
}
