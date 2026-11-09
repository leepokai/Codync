import assert from 'node:assert/strict'
import test from 'node:test'
import { CopyRequests, fitPixels, isPng } from './image-clipboard.ts'

const PNG = Uint8Array.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0])

test('a PNG is known by its signature', () => {
  assert.equal(isPng(PNG), true)
})

test('other bytes are not a PNG, whatever the file is called', () => {
  assert.equal(isPng(Uint8Array.from([0xff, 0xd8, 0xff, 0xe0, 0, 0, 0, 0])), false)
  assert.equal(isPng(PNG.slice(0, 4)), false)
  assert.equal(isPng(new Uint8Array()), false)
})

test('a picture within the cap keeps its size; a bigger one shrinks to fit, in the same shape', () => {
  assert.deepEqual(fitPixels(4000, 3000, 12_000_000), { width: 4000, height: 3000 })
  const fitted = fitPixels(8000, 6000, 12_000_000)
  assert.ok(fitted.width * fitted.height <= 12_000_000)
  assert.ok(Math.abs(fitted.width / fitted.height - 8000 / 6000) < 0.01)
  assert.deepEqual(fitPixels(1, 1, 0), { width: 1, height: 1 })
})

test('a copy overtaken by a newer one is no longer the one to land', () => {
  const requests = new CopyRequests()
  const first = requests.start()
  assert.equal(requests.isLatest(first), true)
  const second = requests.start()
  assert.equal(requests.isLatest(first), false)
  assert.equal(requests.isLatest(second), true)
})
