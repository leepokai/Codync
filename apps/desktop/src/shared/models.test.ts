import assert from 'node:assert/strict'
import { test } from 'node:test'
import { draftOf, isChat, isReplyable, normalizeBot, quoteOf, type Entry } from './models.ts'

test('a bot keeps its Fast mode through the settings draft', () => {
  const bot = normalizeBot({ id: 'b', backend: 'codex', fastMode: true })
  assert.equal(bot.fastMode, true)
  assert.equal(draftOf(bot).fastMode, true)
})

test('a bot without a section, or with a cleared one, has none', () => {
  assert.equal(normalizeBot({ id: 'b', section: 'Work' }).section, 'Work')
  assert.equal(normalizeBot({ id: 'b' }).section, undefined)
  assert.equal(normalizeBot({ id: 'b', section: '' }).section, undefined)
})

test('a host without Fast mode reads as off', () => {
  assert.equal(normalizeBot({ id: 'b' }).fastMode, false)
})

test('the composer offers Fast mode until the agent says it has none, and while it is on', async () => {
  const { fastModeShown, fastModeState, fastModeDetail } = await import('./fast-mode.ts')
  assert.equal(fastModeShown(normalizeBot({ id: 'b' })), true)
  assert.equal(fastModeShown(normalizeBot({ id: 'b', fastModeSupport: 'unsupported' })), false)
  assert.equal(fastModeShown(normalizeBot({ id: 'b', fastModeSupport: 'offered' })), true)
  assert.equal(fastModeShown(normalizeBot({ id: 'b', fastMode: true, fastModeSupport: 'unsupported' })), true)
  assert.equal(fastModeShown(normalizeBot({ id: 'g', kind: 'group', fastModeSupport: 'offered' })), false)
  const refused = normalizeBot({ id: 'b', fastMode: true, fastModeSupport: 'refused', fastModeIssue: 'requires extra usage' })
  assert.equal(fastModeState(refused), 'refused')
  assert.match(fastModeDetail(refused), /requires extra usage/)
})


test('memory lines and summaries stay out of the chat', () => {
  const notice = (data: Entry['data']) => ({ id: 'n', botId: 'b', kind: 'notice', rev: 1, turn: 1, createdAt: 0, data }) as Entry
  assert.equal(isChat(notice({ text: 'Remembered: likes tea', memory: { status: 'done' } })), false)
  assert.equal(isChat(notice({ text: 'Updated people.md', memoryFile: { status: 'done' } })), false)
  assert.equal(isChat(notice({ text: 'Earlier context was summarized to make room.', compaction: { byHost: true } })), false)
  assert.equal(isChat(notice({ text: 'Stopped.' })), true)
})

test('a kept handoff leaves no line; a fresh conversation does', () => {
  const notice = (status: string) => ({ id: 'n', botId: 'b', kind: 'notice', rev: 1, turn: 1, createdAt: 0, data: { text: '…', handoff: { status } } }) as Entry
  assert.equal(isChat(notice('kept')), false)
  assert.equal(isChat(notice('sent')), true)
  assert.equal(isChat(notice('draft')), true)
})

const message = (kind: string, data: Entry['data'], id = 'e1'): Entry => ({ id, seq: 1, botId: 'b', rev: 1, kind, turn: 1, createdAt: 0, updatedAt: 0, data })

test('the user and final bot replies can be quoted, once the computer has them', () => {
  assert.equal(isReplyable(message('user', { text: 'hi' })), true)
  assert.equal(isReplyable(message('agent', { text: 'done', final: true })), true)
  assert.equal(isReplyable(message('agent', { text: 'narration' })), false)
  assert.equal(isReplyable(message('notice', { text: 'Stopped.' })), false)
  assert.equal(isReplyable(message('permission', { title: 'Run?' })), false)
  assert.equal(isReplyable(message('user', { text: 'sending' }, 'local-n')), false)
})

test('a quote names who wrote it and falls back to file names', () => {
  assert.deepEqual(quoteOf(message('user', { text: 'hi' })), { id: 'e1', author: 'user', text: 'hi' })
  assert.equal(quoteOf(message('agent', { text: 'yo', final: true, author: 'ann' })).author, 'ann')
  assert.equal(quoteOf(message('agent', { text: 'yo', final: true })).author, 'b')
  assert.equal(quoteOf(message('user', { text: '', attachments: [{ id: 'a', name: 'a.png', size: 1 }, { id: 'c', name: 'c.pdf', size: 1 }] })).text, 'a.png, c.pdf')
  assert.equal(quoteOf(message('user', { text: 'x'.repeat(900) })).text.length, 500)
})
