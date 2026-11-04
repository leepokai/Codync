import assert from 'node:assert/strict'
import test from 'node:test'
import { imageType } from './image-clipboard.ts'

test('a picture is typed by its extension, whatever its case', () => {
  assert.equal(imageType('Sunset.PNG'), 'image/png')
  assert.equal(imageType('photo.jpeg'), 'image/jpeg')
  assert.equal(imageType('clip.webp'), 'image/webp')
})

test('a name with no known extension has no type', () => {
  assert.equal(imageType('notes'), '')
  assert.equal(imageType('scan.heic'), '')
})
