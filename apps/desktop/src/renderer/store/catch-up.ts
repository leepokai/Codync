import type { HostEvent } from '../client/host-client'

/** Counts a stream's catch-up by the `hello`'s `catchUp`: the host's rev can be ahead of every event it replays. */
export class CatchUp {
  private left: number | null = null

  reset() {
    this.left = null
  }

  /** True once, on the event that ends this stream's catch-up. */
  ended(event: HostEvent): boolean {
    if (event.type === 'hello') this.left = event.catchUp ?? null
    else if (this.left !== null) this.left--
    if (this.left !== 0) return false
    this.left = null
    return true
  }
}
