import type { AnalyticsEvent, ImageAction, ImageFailure, ImageFrom } from '../../shared/analytics.ts'
import { ImageCopyError } from './image-clipboard.ts'

/** What `image_preview_action` reports: what was done, where, and how it ended (`error` only when it failed). */
export function imageActionEvent(action: ImageAction, from: ImageFrom, error?: ImageFailure): AnalyticsEvent {
  return error ? { event: 'image_preview_action', action, from, outcome: 'failed', error } : { event: 'image_preview_action', action, from, outcome: 'done' }
}

/** Which failure a copy that threw reports: the step that failed when it says so, else the clipboard. */
export function copyFailure(error: unknown): ImageFailure {
  return error instanceof ImageCopyError ? error.kind : 'clipboard'
}
