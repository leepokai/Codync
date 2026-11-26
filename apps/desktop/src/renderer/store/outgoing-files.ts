import type { Entry } from '@shared/models'
import { Observable } from '../lib/observable.ts'

/** A file picked in the composer, sent with the next message. */
export interface OutgoingFile {
  id: string
  name: string
  data: Uint8Array
}

export class DraftFiles extends Observable {
  private readonly kept = new Map<string, OutgoingFile[]>()

  get(key: string): OutgoingFile[] {
    return this.kept.get(key) ?? []
  }

  update(key: string, change: (files: OutgoingFile[]) => OutgoingFile[]) {
    const files = change(this.get(key))
    if (files.length) this.kept.set(key, files)
    else this.kept.delete(key)
    this.changed()
  }

  removeBot(id: string) {
    for (const key of this.kept.keys()) {
      if (key === id || key.startsWith(`${id}/`)) this.kept.delete(key)
    }
    this.changed()
  }
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
