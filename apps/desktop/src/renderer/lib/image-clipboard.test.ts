import assert from 'node:assert/strict'
import test from 'node:test'
import { isPng } from './image-clipboard.ts'

const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0])

test('a PNG is known by its signature', () => {
  assert.equal(isPng(PNG), true)
})

test('other bytes are not a PNG, whatever the file is called', () => {
  assert.equal(isPng(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0])), false)
  assert.equal(isPng(PNG.slice(0, 4)), false)
  assert.equal(isPng(new Uint8Array()), false)
})
