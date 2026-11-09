import assert from 'node:assert/strict'
import { test } from 'node:test'
import { pastedFiles, readPicked } from './composer-files.ts'

const png = new File([new Uint8Array([1, 2, 3])], 'shot.png', { type: 'image/png' })
const folder = { name: 'folder', arrayBuffer: () => Promise.reject(new Error('NotFoundError')) } as unknown as File

test('a paste the computer reads keeps its own names and contents', async () => {
  const own = [{ name: 'report.pdf', data: new Uint8Array([9]) }]
  assert.deepEqual(await pastedFiles(async () => own, [png]), own)
})

test('a paste the computer finds nothing in, or fails to read, attaches the files the event held', async () => {
  for (const fromComputer of [async () => [], () => Promise.reject(new Error('clipboard'))]) {
    const files = await pastedFiles(fromComputer, [png])
    assert.deepEqual(files.map((f) => [f.name, [...f.data]]), [['shot.png', [1, 2, 3]]])
  }
})

test('an unreadable item (a folder) is left out and an unnamed image gets a name', async () => {
  const unnamed = new File([new Uint8Array([4])], '', { type: 'image/png' })
  const files = await readPicked([folder, unnamed])
  assert.equal(files.length, 1)
  assert.match(files[0].name, /^pasted-\d+\.png$/)
})
