import assert from 'node:assert/strict'
import test from 'node:test'
import type { Entry } from '../../../shared/models.ts'
import { copiedPicture, leavingViewer } from './message-copy.ts'

const entry = (data: Partial<Entry['data']>) => ({ id: 'e1', botId: 'b1', kind: 'agent', data }) as unknown as Entry
const shared = { id: 'f1', name: 'chart.png', size: 10, sha256: '0'.repeat(64) }
const pdf = { ...shared, id: 'f2', name: 'report.pdf' }
const sent = { id: 'u1', name: 'me.jpg', size: 10 }

test('a message with no picture copies its text', () => {
  assert.equal(copiedPicture(entry({ text: 'hello' })), null)
  assert.equal(copiedPicture(entry({ files: [pdf] })), null)
})

test('a picture a bot shared on its own copies as the picture, even with a caption', () => {
  assert.deepEqual(copiedPicture(entry({ text: 'chart.png', files: [shared] })), { file: shared, entryId: 'e1' })
})

test('with several files, each picture has its own menu and the message keeps copying its text', () => {
  assert.equal(copiedPicture(entry({ text: 'two', files: [pdf, shared] })), null)
  assert.equal(copiedPicture(entry({ attachments: [sent, { ...sent, id: 'u2' }] })), null)
})

test('a sent picture with no text copies as the picture; with text the text stays the copy', () => {
  assert.deepEqual(copiedPicture(entry({ attachments: [sent] })), { file: sent, botId: 'b1' })
  assert.equal(copiedPicture(entry({ text: 'look', attachments: [sent] })), null)
})

test('in the viewer a menu item closes the viewer before it acts, and the reactions are untouched', () => {
  const order: string[] = []
  const reactions = { emoji: ['👍'], chosen: [], toggle: () => order.push('react') }
  const menu = leavingViewer({ items: () => [{ title: 'Reply', action: () => order.push('reply') }], reactions }, () => order.push('close'))
  menu.items()[0].action?.()
  assert.deepEqual(order, ['close', 'reply'])
  assert.equal(menu.reactions, reactions)
})
