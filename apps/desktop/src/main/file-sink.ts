import { createHash } from 'node:crypto'
import { open, rename, rm, type FileHandle } from 'node:fs/promises'
import type { SharedFile } from '../shared/models'
import { FILE_CHUNK_SIZE, validateSharedFile } from '../shared/files.ts'

/** Only verified complete bytes replace the destination; every failure removes the partial. */
export class FileSink {
  private hash = createHash('sha256')
  private offset = 0
  private closed = false
  private busy = false
  private file: FileHandle
  private partial: string
  private destination: string
  private meta: SharedFile
  private constructor(file: FileHandle, partial: string, destination: string, meta: SharedFile) {
    this.file = file; this.partial = partial; this.destination = destination; this.meta = meta
  }

  static async create(destination: string, meta: SharedFile, id: string) {
    validateSharedFile(meta)
    const partial = `${destination}.${id}.codync-partial`
    const file = await open(partial, 'wx', 0o600)
    return new FileSink(file, partial, destination, meta)
  }

  async write(offset: number, bytes: Uint8Array) {
    if (this.closed || this.busy || offset !== this.offset || bytes.length !== Math.min(FILE_CHUNK_SIZE, this.meta.size - offset)) throw new Error('Invalid file chunk')
    this.busy = true
    try {
      let written = 0
      while (written < bytes.length) {
        const result = await this.file.write(bytes, written, bytes.length - written, this.offset + written)
        if (!result.bytesWritten) throw new Error('Could not write file')
        written += result.bytesWritten
      }
      if (this.closed) throw new Error('Download cancelled')
      this.hash.update(bytes)
      this.offset += bytes.length
    } finally { this.busy = false }
  }

  async finish() {
    if (this.closed || this.busy) throw new Error('Download cancelled or still writing')
    if (this.offset !== this.meta.size || this.hash.digest('hex') !== this.meta.sha256) throw new Error('File integrity check failed; retry the download')
    await this.file.sync()
    if (this.closed) throw new Error('Download cancelled')
    await this.file.close()
    if (this.closed) throw new Error('Download cancelled')
    // No asynchronous work between the last cancellation check and starting the rename.
    await rename(this.partial, this.destination)
    this.closed = true
  }

  async cancel() {
    if (this.closed) return
    this.closed = true
    await this.file.close()
    await rm(this.partial, { force: true })
  }
}
