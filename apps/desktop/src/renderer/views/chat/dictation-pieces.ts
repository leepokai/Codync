/** Under half a second is a click of the mic, not speech (and speech-to-text models invent words for silence). */
export const MIN_SAMPLES = 16000 / 2
/** One transcription request: 20 s of 16 kHz audio, well under the host's 2 MB request body once base64-encoded. */
export const PIECE_SAMPLES = 16000 * 20
/** Transcription requests running at once, under the providers' burst limits. */
export const IN_FLIGHT = 3

/** `said` after what's already in the box, with one space between. */
export function joinDictated(before: string, said: string) {
  if (!before.trim()) return said
  if (!said) return before
  return /\s$/.test(before) ? before + said : `${before} ${said}`
}

/** A recording cut into requests of `size` samples; a tail under `MIN_SAMPLES` stays with the piece before. */
export function pieces(chunks: Int16Array[], size = PIECE_SAMPLES): Int16Array[] {
  const all = new Int16Array(chunks.reduce((n, c) => n + c.length, 0))
  let at = 0
  for (const c of chunks) {
    all.set(c, at)
    at += c.length
  }
  const out: Int16Array[] = []
  for (let start = 0; start < all.length; start += size) {
    const end = all.length - (start + size) < MIN_SAMPLES ? all.length : start + size
    out.push(all.subarray(start, end))
    if (end === all.length) break
  }
  return out
}

/** `run` over `items`, at most `limit` at once, results in the items' order; the first failure stops the rest. */
export async function inOrder<T, R>(items: T[], run: (item: T) => Promise<R>, limit = IN_FLIGHT): Promise<R[]> {
  const out: R[] = []
  let next = 0
  let failed = false
  const worker = async () => {
    while (!failed && next < items.length) {
      const i = next++
      try {
        out[i] = await run(items[i]!)
      } catch (e) {
        failed = true
        throw e
      }
    }
  }
  await Promise.all(Array.from({ length: Math.min(limit, items.length) }, worker))
  return out
}
