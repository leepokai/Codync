import type { HostClient } from '../../client/host-client'
import { errorText } from '../../lib/error-text'
import { Observable } from '../../lib/observable'
import { trNow } from '../../lib/l10n'
import { localDictation, setSpeechServer, type LocalDictation } from './voice-settings'

/**
 * The computer's speech server for the This computer card: whether one is linked and linking or unlinking it.
 * `client` is the live client, null while the computer isn't connected.
 */
export class LocalDictationModel extends Observable {
  /** The computer's answer; null until it gives one. */
  local: LocalDictation | null = null
  /** Why the card can't show the computer's answer, or what to do without a server. */
  note: string | null = null
  busy = false
  /** Why the last link or unlink failed. */
  failure: string | null = null

  constructor(
    private readonly client: () => HostClient | null,
    private readonly hostName: () => string,
  ) {
    super()
  }

  /** Asks the computer again; the card calls it whenever the computer (re)connects. */
  async load() {
    const client = this.client()
    if (!client) return this.update({ note: trNow('Not connected to {name}.').replaceAll('{name}', this.hostName) })
    try {
      this.show(await localDictation(client))
    } catch (e) {
      this.update({ note: errorText(e) })
    }
  }

  /**
   * The computer tries a transcription on `url` (asking for `model`, if any) before it keeps it; an empty `url` unlinks.
   * Resolves to whether it did, so the card clears what was typed.
   */
  async link(url: string, model = '') {
    const client = this.client()
    if (this.busy || !client) return false
    this.update({ busy: true, failure: null })
    try {
      this.show(await setSpeechServer(client, url, model))
      return true
    } catch (e) {
      this.update({ failure: errorText(e) })
      return false
    } finally {
      this.update({ busy: false })
    }
  }

  private show(local: LocalDictation) {
    this.update({ local, note: local.ready ? null : local.message })
  }

  private update(change: Partial<Pick<LocalDictationModel, 'local' | 'note' | 'busy' | 'failure'>>) {
    Object.assign(this, change)
    this.changed()
  }
}
