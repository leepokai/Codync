/** A file picked in the composer, sent with the next message. */
export interface OutgoingFile {
  id: string
  name: string
  data: Uint8Array
}

/** Anything larger is refused by the computer. */
export const MAX_FILE_SIZE = 100 * 1024 * 1024
