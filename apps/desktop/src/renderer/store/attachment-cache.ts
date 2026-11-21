/** Bytes the attachment cache keeps before its oldest files go (64 MiB): a chat's images and sounds are read from
 *  the computer again after that, so browsing a long history cannot hold hundreds of megabytes. */
export const CACHE_LIMIT = 64 * 1024 * 1024

const cached = new Map<string, Uint8Array>()
let held = 0

/** The bytes kept for `id`, marked as the most recently used; undefined when they were let go. */
export function cachedAttachment(id: string): Uint8Array | undefined {
  const data = cached.get(id)
  if (!data) return undefined
  cached.delete(id)
  cached.set(id, data)
  return data
}

/** Keeps `data` for `id` and lets the oldest files go, so the cache stays under its limit. */
export function cacheAttachment(id: string, data: Uint8Array) {
  const old = cached.get(id)
  if (old) {
    held -= old.byteLength
    cached.delete(id)
  }
  if (data.byteLength > CACHE_LIMIT) return
  cached.set(id, data)
  held += data.byteLength
  for (const [key, value] of cached) {
    if (held <= CACHE_LIMIT) break
    cached.delete(key)
    held -= value.byteLength
  }
}
