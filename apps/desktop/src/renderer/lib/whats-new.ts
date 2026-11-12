import file from '../../../../ios/Kit/Sources/CodyncKit/Resources/Announcements/announcements.json' with { type: 'json' }
import { isBelow } from '../../shared/compat.ts'

/** An important release's highlights: English phrases, translated like the rest of the interface (docs/features/whats-new.md). */
export interface Announcement {
  version: string
  notes: string[]
}

/** The file every client reads, newest first. */
export const announcements: Announcement[] = file

/**
 * What a device on `current` that last opened on `seen` hasn't been shown: `seen < version ≤ current`, in the file's
 * order (newest first). Without `seen`, a device set up before this feature (`usedBefore`) counts from 0.0.0 and a
 * new install from `current`.
 */
export function due(all: Announcement[], seen: string | null, current: string, usedBefore: boolean): Announcement[] {
  const since = seen ?? (usedBefore ? '0.0.0' : current)
  return all.filter((a) => isBelow(since, a.version) && !isBelow(current, a.version))
}
