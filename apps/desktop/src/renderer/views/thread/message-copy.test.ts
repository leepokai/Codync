import assert from 'node:assert/strict'
import test from 'node:test'
import type { Entry } from '../../../shared/models.ts'
import { copiedPicture } from './message-copy.ts'

const entry = (data: Partial<Entry['data']>) => ({ id: 'e1', botId: 'b1', kind: 'agent', data }) as unknown as Entry
const shared = { id: 'f1', name: 'chart.png', size: 10, sha256: '0'.repeat(64) }
const sent = { id: 'u1', name: 'me.jpg', size: 10 }

test('a message with no picture copies its text', () => {
  assert.equal(copiedPicture(entry({ text: 'hello' })), null)
  // A shared file that is not a picture is not one either.
  assert.equal(copiedPicture(entry({ files: [{ ...shared, name: 'report.pdf' }] })), null)
})

test('a picture a bot shared copies as the picture, even with a caption', () => {
  assert.deepEqual(copiedPicture(entry({ text: 'chart.png', files: [shared] })), { file: shared, entryId: 'e1' })
})

test('a sent picture with no text copies as the picture; with text the text stays the copy', () => {
  assert.deepEqual(copiedPicture(entry({ attachments: [sent] })), { file: sent, botId: 'b1' })
  assert.equal(copiedPicture(entry({ text: 'look', attachments: [sent] })), null)
})
