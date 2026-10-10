import assert from 'node:assert/strict'
import { test } from 'node:test'
import { createHash, randomUUID } from 'node:crypto'
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { FileSink } from '../main/file-sink.ts'
import { decodeFileChunk, FILE_CHUNK_SIZE, saveName } from './files.ts'

test('verified downloads preserve binary and empty files and replace only on success', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'codync-download-'))
  try {
    for (const data of [Buffer.alloc(0), Buffer.alloc(FILE_CHUNK_SIZE + 11, 255)]) {
      const destination = join(directory, 'output')
      await writeFile(destination, 'original')
      const sink = await FileSink.create(destination, { id: 'id', name: 'file', size: data.length, sha256: createHash('sha256').update(data).digest('hex') }, randomUUID())
      for (let offset = 0; offset < data.length; offset += FILE_CHUNK_SIZE) await sink.write(offset, data.subarray(offset, offset + FILE_CHUNK_SIZE))
      const original = await readFile(destination, 'utf8')
      assert.equal(original, 'original')
      await sink.finish()
      const saved = await readFile(destination)
      assert.deepEqual(saved, data)
      const remaining = await readdir(directory)
      assert.deepEqual(remaining, ['output'])
    }
  } finally { await rm(directory, { recursive: true, force: true }) }
})

test('cancelled and corrupt downloads preserve existing destinations and remove partial files', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'codync-download-'))
  try {
    const destination = join(directory, 'output')
    await writeFile(destination, 'original')
    for (const corrupt of [false, true]) {
      const sink = await FileSink.create(destination, { id: 'id', name: '.hidden', size: 1, sha256: '0'.repeat(64) }, randomUUID())
      await sink.write(0, new Uint8Array([42]))
      if (corrupt) await assert.rejects(sink.finish(), /integrity/)
      await sink.cancel()
      const original = await readFile(destination, 'utf8')
      assert.equal(original, 'original')
      const remaining = await readdir(directory)
      assert.deepEqual(remaining, ['output'])
    }
  } finally { await rm(directory, { recursive: true, force: true }) }
})

test('rejects changed, truncated and oversized chunks and provides portable save suggestions', () => {
  assert.throws(() => decodeFileChunk({ data: '', size: 1 }, 1, 0), /truncated/)
  assert.throws(() => decodeFileChunk({ data: 'AA==', size: 2 }, 1, 0), /Invalid/)
  assert.throws(() => decodeFileChunk({ data: 'A'.repeat(FILE_CHUNK_SIZE * 2), size: 1 }, 1, 0), /Invalid/)
  assert.equal(saveName('CON.txt'), 'file-CON.txt')
  assert.equal(saveName('file:report?.pdf'), 'file_report_.pdf')
  assert.equal(saveName('.hidden'), '.hidden')
})

test('retiring a computer while Save is pending cancels its handle without fetching bytes', async () => {
  const { FileDownloads } = await import('../renderer/store/file-downloads.ts')
  let resolve!: (value: string) => void
  const pending = new Promise<string>((done) => { resolve = done })
  const cancelled: string[] = []
  Object.defineProperty(globalThis, 'window', { configurable: true, value: { codync: { files: {
    begin: () => pending, cancel: async (id: string) => { cancelled.push(id) },
  } } } })
  const downloads = new FileDownloads(() => {})
  let calls = 0
  const client = { readFile: async () => { calls++; return { data: '', size: 0 } } }
  const task = downloads.save(client as never, 'entry', { id: 'id', name: 'empty', size: 0, sha256: createHash('sha256').digest('hex') })
  downloads.retire()
  resolve('handle')
  await task
  assert.equal(calls, 0)
  assert.deepEqual(cancelled, ['handle'])
  assert.equal(downloads.states.has('id'), false)
})

test('cancelling an outstanding chunk cannot write or commit a late response', async () => {
  const { FileDownloads } = await import('../renderer/store/file-downloads.ts')
  let resolve!: (value: { data: string; size: number }) => void
  const pending = new Promise<{ data: string; size: number }>((done) => { resolve = done })
  let writes = 0, commits = 0
  Object.defineProperty(globalThis, 'window', { configurable: true, value: { codync: { files: {
    begin: async () => 'handle', cancel: async () => {}, write: async () => { writes++ }, finish: async () => { commits++ },
  } } } })
  const downloads = new FileDownloads(() => {})
  const task = downloads.save({ readFile: () => pending } as never, 'entry', { id: 'id', name: 'empty', size: 0, sha256: createHash('sha256').digest('hex') })
  await new Promise((done) => setImmediate(done))
  downloads.cancel('id')
  resolve({ data: '', size: 0 })
  await task
  assert.equal(writes, 0)
  assert.equal(commits, 0)
  assert.equal(downloads.states.has('id'), false)
})
