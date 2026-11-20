import file from '../../../../ios/Kit/Sources/CodyncUI/Resources/Announcements/announcements.json' with { type: 'json' }
import { isBelow } from '../../shared/compat.ts'

/** One highlight: a short preview clip over its name and a sentence about it. */
export interface Feature {
  /** English phrases, translated like the rest of the interface. */
  title: string
  body: string
  /** Still in beta: a tag beside the title. */
  beta?: boolean
  /** The name of its muted, looping clip (`<media>.mp4` beside the file). */
  media?: string
  /** Where the full video plays. */
  film?: string
}

/** An important release's highlights (docs/features/whats-new.md). */
export interface Announcement {
  version: string
  features?: Feature[]
  /** Smaller changes, text only: English phrases, translated like the rest of the interface. */
  notes?: string[]
}

/** The file every client reads, newest first. */
export const announcements: Announcement[] = file

/** Every phrase the window shows for these, for the catalogs. */
export const phrases = (all: Announcement[]): string[] =>
  all.flatMap((a) => [...(a.features ?? []).flatMap((f) => [f.title, f.body]), ...(a.notes ?? [])])

/**
 * What a device on `current` that last opened on `seen` hasn't been shown: `seen < version ≤ current`, in the file's
 * order (newest first). Without `seen`, a device set up before this feature (`usedBefore`) counts from 0.0.0 and a
 * new install from `current`.
 */
export function due(all: Announcement[], seen: string | null, current: string, usedBefore: boolean): Announcement[] {
  const since = seen ?? (usedBefore ? '0.0.0' : current)
  return all.filter((a) => isBelow(since, a.version) && !isBelow(current, a.version))
}
