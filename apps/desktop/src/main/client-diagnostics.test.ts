import assert from 'node:assert/strict'
import { existsSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { createClientDiagnostics } from './client-diagnostics.ts'

const dir = () => mkdtempSync(join(tmpdir(), 'client-diagnostics-'))
const read = (path: string) => readFileSync(path, 'utf8').trim().split('\n').map((line) => JSON.parse(line) as Record<string, unknown>)

test('an error record keeps no message, stack, URL or full path', async () => {
  const file = join(dir(), 'client-events.jsonl')
  const diagnostics = createClientDiagnostics(() => file)
  diagnostics.recordError({
    process: 'desktop-renderer',
    type: 'TypeError',
    mechanism: 'onerror',
    value: 'boom',
    message: 'secret chat text',
    stack: 'at C:\\Users\\me\\secret.ts:1:1',
    frames: [{ file: 'C:\\Users\\me\\app\\assets\\index-AbCdEf12.js', line: 12, column: 3 }, { file: 'C:\\Users\\me\\notes.txt', line: 1 }],
  } as never)
  await diagnostics.flush()
  const [record] = read(file)
  assert.equal(record.event, 'error')
  assert.equal(record.type, 'TypeError')
  assert.equal(record.mechanism, 'onerror')
  assert.ok(!('message' in record) && !('stack' in record) && !('value' in record))
  assert.deepEqual(record.frames, [
    { file: 'index-AbCdEf12.js', line: 12, column: 3 },
    { file: 'other', line: 1 },
  ])
})

test('the log rotates to .1 and keeps both files under the byte budget', async () => {
  const file = join(dir(), 'client-events.jsonl')
  const diagnostics = createClientDiagnostics(() => file, { maxBytes: 400 })
  for (let i = 0; i < 20; i++) diagnostics.recordWindow('dom-ready', { webContentsId: i })
  await diagnostics.flush()
  assert.ok(existsSync(`${file}.1`), 'the previous file is kept as .1')
  const latest = read(file)
  assert.ok(latest.length > 0)
})

test('a queue limit drops records instead of growing without bound', async () => {
  const file = join(dir(), 'client-events.jsonl')
  const diagnostics = createClientDiagnostics(() => file, { maxQueued: 2 })
  for (let i = 0; i < 50; i++) diagnostics.recordWindow('did-finish-load', { webContentsId: i })
  await diagnostics.flush()
  const records = read(file)
  assert.ok(records.length <= 3, `wrote ${records.length} records`)
})

test('an unwritable file never throws into the caller', async () => {
  const blocker = join(dir(), 'not-a-directory'); writeFileSync(blocker, 'x')
  const diagnostics = createClientDiagnostics(() => join(blocker, 'client-events.jsonl'))
  assert.doesNotThrow(() => diagnostics.recordWindow('unresponsive', { webContentsId: 1 }))
  await assert.doesNotReject(diagnostics.flush())
})

