import type { SharedFile } from './models'

export const FILE_CHUNK_SIZE = 384 * 1024
export const FILE_LIMIT = 100 * 1024 * 1024

export function validateSharedFile(file: SharedFile) {
  if (!Number.isSafeInteger(file.size) || file.size < 0 || file.size > FILE_LIMIT || !/^[a-f0-9]{64}$/.test(file.sha256)) throw new Error('Invalid file metadata')
  if (!file.name || file.name.length > 200 || /[/\\\x00-\x1f\x7f]/.test(file.name) || file.name === '.' || file.name === '..') throw new Error('Invalid filename')
}

/** A display filename is a suggestion to the native dialog, never a storage path. */
export function saveName(name: string) {
  const portable = name.replace(/[<>:"|?*]/g, '_').replace(/[. ]+$/, '')
  return portable && !/^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\.|$)/i.test(portable) ? portable : `file-${portable || 'download'}`
}

export function decodeFileChunk(chunk: { data: string; size: number }, size: number, offset: number): Uint8Array {
  if (chunk.size !== size || typeof chunk.data !== 'string' || chunk.data.length > Math.ceil(FILE_CHUNK_SIZE / 3) * 4) throw new Error('Invalid download response')
  const bytes = Uint8Array.from(atob(chunk.data), (v) => v.charCodeAt(0))
  if (bytes.length !== Math.min(FILE_CHUNK_SIZE, size - offset)) throw new Error('Download was truncated or changed')
  return bytes
}
