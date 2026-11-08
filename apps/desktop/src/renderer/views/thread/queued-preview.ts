import { quoteOf, type Entry } from '@shared/models'

/** What a waiting message says, for the strip and the queue list: its text, or the names of what it carries. */
export function previewOf(entry: Entry): string {
  const preview = quoteOf(entry).text.trim()
  return preview || 'Attachment'
}
