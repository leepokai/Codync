import type { EventEmitter } from 'node:events'

/** Keeps ordinary window closes in the background, but lets installers exit. */
export class QuitLifecycle {
  private quitting = false

  constructor(app: Pick<EventEmitter, 'on'>, updater: Pick<EventEmitter, 'on'>) {
    app.on('before-quit', () => this.beginQuit())
    // Native macOS updates close windows BEFORE app's before-quit event.
    updater.on('before-quit-for-update', () => this.beginQuit())
  }

  beginQuit() {
    this.quitting = true
  }

  closeToBackground(event: { preventDefault(): void }, hide: () => void) {
    if (this.quitting) return
    event.preventDefault()
    hide()
  }
}
