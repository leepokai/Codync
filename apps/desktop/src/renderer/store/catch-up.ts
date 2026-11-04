import type { HostEvent } from '../client/host-client'

/** The `hello`'s `catchUp`, or undefined from a host before the count. */
export function readCatchUp(raw: Record<string, unknown>): number | undefined {
  return typeof raw.catchUp === 'number' && raw.catchUp >= 0 ? raw.catchUp : undefined
}

/** Counts a stream's catch-up by the `hello`'s `catchUp`: the host's rev can be ahead of every event it replays. */
export class CatchUp {
  private left: number | null = null

  reset() {
    this.left = null
  }

  /** Between a counted `hello` and its last replayed event: what's here may still be behind the host. */
  get replaying(): boolean {
    return this.left !== null && this.left > 0
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
