/**
 * Holds a device link's outgoing messages to the relay's per-link budget (cloud `relay-wire.ts` `RATE_*`: ≤ 200 messages and
 * ≤ 12 MiB per 10 s), which closes the link when exceeded. The window here is a second longer, so messages still in
 * flight never land in the relay's next window early. Messages leave in the order they were queued.
 *
 * Each send is a function that makes its messages only once the window has room for the first of them, so what it
 * makes (a sealed frame's counter, a request's id) starts the moment it leaves, and it can still return nothing for a
 * request given up on while it waited.
 *
 * Paced messages leave the window's last `HANDSHAKE_*` free, so a handshake (`sendFirst`) leaves at once even when they
 * fill it: the computer closes a link whose `hello` is late.
 */
const WINDOW_MS = 11_000
const MAX_MESSAGES = 200
const MAX_BYTES = 12 * 1024 * 1024
/** Room for a few handshakes in one window (a computer rejoining again and again); a `hello` is under 1 KiB. */
const HANDSHAKE_MESSAGES = 8
const HANDSHAKE_BYTES = 64 * 1024
/** The largest WebSocket message Cloudflare passes; a sealed 256 KiB frame is about 350 KiB. */
const MAX_MESSAGE_BYTES = 1024 * 1024

export class RelayPacer {
  private queue: (() => string[])[] = []
  /** Handshakes waiting: only once earlier ones used up the full window. */
  private first: string[] = []
  /** Made, not sent yet: the rest of a message sealed into several frames. */
  private made: string[] = []
  private sent: { at: number; bytes: number }[] = []
  private timer: NodeJS.Timeout | undefined

  private readonly output: (text: string) => void

  constructor(output: (text: string) => void) {
    this.output = output
  }

  send(make: () => string[]) {
    this.queue.push(make)
    if (!this.timer) this.drain()
  }

  /** Sends `text` ahead of everything waiting, in the room paced messages leave free. */
  sendFirst(text: string) {
    this.first.push(text)
    this.drain()
  }

  /** Drops what hasn't left yet (the channel it was sealed for is gone); what left still counts against the window. */
  clear() {
    this.queue = []
    this.first = []
    this.made = []
    clearTimeout(this.timer)
    this.timer = undefined
  }

  private drain() {
    clearTimeout(this.timer)
    this.timer = undefined
    while (this.first.length || this.made.length || this.queue.length) {
      if (this.first.length) {
        const size = Buffer.byteLength(this.first[0]!)
        if (this.fits(size, MAX_MESSAGES, MAX_BYTES)) {
          this.write(this.first.shift()!, size)
          continue
        }
      } else {
        const size = this.made.length ? Buffer.byteLength(this.made[0]!) : MAX_MESSAGE_BYTES
        if (this.fits(size, MAX_MESSAGES - HANDSHAKE_MESSAGES, MAX_BYTES - HANDSHAKE_BYTES)) {
          if (!this.made.length) this.made = this.queue.shift()!()
          else this.write(this.made.shift()!, size)
          continue
        }
      }
      this.timer = setTimeout(() => this.drain(), this.sent[0]!.at + WINDOW_MS - Date.now())
      return
    }
  }

  /** Whether one more message of `bytes` stays within `messages` and `maxBytes` for the current window. */
  private fits(bytes: number, messages: number, maxBytes: number) {
    const now = Date.now()
    this.sent = this.sent.filter((s) => now - s.at < WINDOW_MS)
    return this.sent.length < messages && bytes <= maxBytes - this.sent.reduce((sum, s) => sum + s.bytes, 0)
  }

  private write(text: string, bytes: number) {
    this.sent.push({ at: Date.now(), bytes })
    this.output(text)
  }
}
