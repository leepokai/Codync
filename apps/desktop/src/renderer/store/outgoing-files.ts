import type { Entry } from '@shared/models'

/** A file picked in the composer, sent with the next message. */
export interface OutgoingFile {
  id: string
  name: string
  data: Uint8Array
}

/** Anything larger is refused by the computer. */
export const MAX_FILE_SIZE = 100 * 1024 * 1024

/**
 * The files Resend sends again, or null when the message had files that are gone: they're kept only in
 * memory, so after a relaunch it would go half sent.
 */
export function filesToResend(entry: Entry, kept: ReadonlyMap<string, OutgoingFile[]>): OutgoingFile[] | null {
  const files = entry.data.clientNonce ? kept.get(entry.data.clientNonce) : undefined
  return files ?? (entry.data.attachments?.length ? null : [])
}
