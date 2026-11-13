/**
 * Holds a device link's outgoing messages to the relay's per-link budget (cloud `relay-wire.ts` `RATE_*`: ≤ 200 messages and
 * ≤ 12 MiB per 10 s), which closes the link when exceeded. The window here is a second longer, so messages still in
 * flight never land in the relay's next window early. Messages leave in the order they were queued.
 *
 * Each send is a function that makes its messages only once the window has room for the first of them, so what it
 * makes (a sealed frame's counter, a request's id, a deadline) starts the moment it leaves, and it can still return
 * nothing for a request given up on while it waited.
 */
const WINDOW_MS = 11_000
const MAX_MESSAGES = 200
const MAX_BYTES = 12 * 1024 * 1024
/** The largest WebSocket message Cloudflare passes; a sealed 256 KiB frame is about 350 KiB. */
const MAX_MESSAGE_BYTES = 1024 * 1024

export class RelayPacer {
  private queue: (() => string[])[] = []
  /** Made, not sent yet: the rest of a message sealed into several frames. */
  private made: string[] = []
  private sent: { at: number; bytes: number }[] = []
  private timer: NodeJS.Timeout | undefined

  private readonly write: (text: string) => void

  constructor(write: (text: string) => void) {
    this.write = write
  }

  send(make: () => string[]) {
    this.queue.push(make)
    if (!this.timer) this.drain()
  }

  /** Drops what hasn't left yet (the channel it was sealed for is gone); what left still counts against the window. */
  clear() {
    this.queue = []
    this.made = []
    clearTimeout(this.timer)
    this.timer = undefined
  }

  private drain() {
    this.timer = undefined
    while (this.made.length || this.queue.length) {
      const now = Date.now()
      this.sent = this.sent.filter((s) => now - s.at < WINDOW_MS)
      const room = MAX_BYTES - this.sent.reduce((sum, s) => sum + s.bytes, 0)
      const size = this.made.length ? Buffer.byteLength(this.made[0]!) : MAX_MESSAGE_BYTES
      if (this.sent.length >= MAX_MESSAGES || size > room) {
        // A drain run from inside `write` (a reply that sends again) may have set one already.
        clearTimeout(this.timer)
        this.timer = setTimeout(() => this.drain(), this.sent[0]!.at + WINDOW_MS - now)
        return
      }
      if (!this.made.length) {
        this.made = this.queue.shift()!()
        continue
      }
      const text = this.made.shift()!
      this.sent.push({ at: now, bytes: size })
      this.write(text)
    }
  }
}
