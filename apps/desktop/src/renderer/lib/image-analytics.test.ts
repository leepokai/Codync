import assert from 'node:assert/strict'
import test from 'node:test'
import { copyFailure, imageActionEvent } from './image-analytics.ts'
import { ImageCopyError } from './image-clipboard.ts'

test('a finished action reports done with where it was done, and no error', () => {
  assert.deepEqual(imageActionEvent('copy_name', 'menu'), { event: 'image_preview_action', action: 'copy_name', from: 'menu', outcome: 'done' })
})

test('a failed action reports its category', () => {
  assert.deepEqual(imageActionEvent('download', 'viewer', 'transfer'), {
    event: 'image_preview_action', action: 'download', from: 'viewer', outcome: 'failed', error: 'transfer',
  })
})

test('a copy that threw reports the step that failed, else the clipboard', () => {
  assert.equal(copyFailure(new ImageCopyError('convert', null)), 'convert')
  assert.equal(copyFailure(new ImageCopyError('clipboard', null)), 'clipboard')
  assert.equal(copyFailure(new Error('x')), 'clipboard')
  assert.equal(copyFailure(null), 'clipboard')
})
