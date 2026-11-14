import type { AnalyticsEvent } from '@shared/analytics'

/** Sends one event; rejects when the host can't be reached, so the event waits for the next try. */
export type AnalyticsSender = (event: AnalyticsEvent) => Promise<void>

/**
 * App-level events held until this computer's host answers, then sent one at a time in the order
 * they happened. Past `limit` waiting, the oldest go.
 */
export class AnalyticsQueue {
  private pending: AnalyticsEvent[] = []
  private sender: AnalyticsSender | null = null
  private sending = false
  private readonly limit: number

  constructor(limit = 100) {
    this.limit = limit
  }

  track(event: AnalyticsEvent) {
    this.pending.push(event)
    if (this.pending.length > this.limit) this.pending.shift()
    void this.flush()
  }

  /** The host answered (how to reach it), or is gone (null). */
  connect(sender: AnalyticsSender | null) {
    this.sender = sender
    void this.flush()
  }

  private async flush() {
    if (this.sending) return
    this.sending = true
    try {
      for (let next = this.pending[0]; next && this.sender; next = this.pending[0]) {
        await this.sender(next)
        // Unless the limit already dropped it while it was on its way.
        if (this.pending[0] === next) this.pending.shift()
      }
    } catch {
      // The host can't be reached: the event stays first in line for the next connect or event.
    } finally {
      this.sending = false
    }
  }
}
