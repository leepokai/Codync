/**
 * Holds a device link's outgoing messages to the relay's per-link budget (cloud `relay.ts` `charge`: ≤ 200 messages and
 * ≤ 12 MiB per 10 s), which closes the link when exceeded. The window here is a second longer, so messages still in
 * flight never land in the relay's next window early. Messages leave in the order they were queued.
 */
const WINDOW_MS = 11_000
const MAX_MESSAGES = 200
const MAX_BYTES = 12 * 1024 * 1024

export class RelayPacer {
  private queue: string[] = []
  private sent: { at: number; bytes: number }[] = []
  private timer: NodeJS.Timeout | undefined

  private readonly write: (text: string) => void

  constructor(write: (text: string) => void) {
    this.write = write
  }

  send(text: string) {
    this.queue.push(text)
    if (!this.timer) this.drain()
  }

  /** Drops what hasn't left yet: the link is gone. */
  stop() {
    this.queue = []
    clearTimeout(this.timer)
    this.timer = undefined
  }

  private drain() {
    this.timer = undefined
    const now = Date.now()
    this.sent = this.sent.filter((s) => now - s.at < WINDOW_MS)
    let bytes = this.sent.reduce((sum, s) => sum + s.bytes, 0)
    while (this.queue.length) {
      const size = Buffer.byteLength(this.queue[0]!)
      if (this.sent.length >= MAX_MESSAGES || bytes + size > MAX_BYTES) {
        this.timer = setTimeout(() => this.drain(), this.sent[0]!.at + WINDOW_MS - now)
        return
      }
      this.write(this.queue.shift()!)
      this.sent.push({ at: now, bytes: size })
      bytes += size
    }
  }
}
