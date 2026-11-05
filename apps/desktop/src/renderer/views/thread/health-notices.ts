import type { Entry } from '@shared/models'

/**
 * Only the newest memory banner stays: before the host coalesced them, a reading
 * that hovered at a threshold wrote one per sample, and old caches still hold
 * them. Other notices are untouched.
 */
export function newestHealthNotices(entries: Entry[]): Entry[] {
  const isHealth = (e: Entry) =>
    e.kind === 'notice' &&
    (e.data.health === true || (typeof e.data.text === 'string' && e.data.text.startsWith("This computer's memory is")))
  let last = -1
  for (let i = 0; i < entries.length; i++) if (isHealth(entries[i])) last = i
  return last < 0 ? entries : entries.filter((e, i) => !isHealth(e) || i === last)
}
