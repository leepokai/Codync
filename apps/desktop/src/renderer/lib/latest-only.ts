/**
 * Sends one value at a time, and only the latest of those that came in meanwhile. A host handles requests side by
 * side, so two sent together could land in either order. Every call's promise settles when the sending is done; a
 * failure drops what was waiting, since the caller goes back to what the host has.
 */
export function latestOnly<T>(send: (value: T) => Promise<void>): (value: T) => Promise<void> {
  let waiting: { value: T } | undefined
  let sending: Promise<void> | undefined
  const drain = async () => {
    try {
      while (waiting) {
        const { value } = waiting
        waiting = undefined
        await send(value)
      }
    } finally {
      waiting = undefined
      sending = undefined
    }
  }
  return (value) => {
    waiting = { value }
    sending ??= drain()
    return sending
  }
}
