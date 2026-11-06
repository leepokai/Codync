import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { MARK_UNREAD_HOST, offersMarkUnread, ReadMarks } from './read-marks.ts'

function computer(bot: Partial<Bot>, answer: (bots: Map<string, Bot>) => unknown = () => ({})) {
  const bots = new Map([['a', { id: 'a', rev: 1, unread: 0, keptUnread: false, ...bot } as Bot]])
  const sent: unknown[][] = []
  const answers: ((accepted: boolean) => void)[] = []
  const errors: string[] = []
  const client = { call: async (method: string, body: unknown) => {
    sent.push([method, body])
    return answer(bots)
  } } as unknown as HostClient
  const reads = new ReadMarks({
    bots: () => bots,
    changed: () => {},
    put: (held) => bots.set(held.id, held),
    perform: (op, undo) => new Promise<boolean>((resolve) => {
      void op(client)
      answers.push((accepted) => {
        if (!accepted) {
          undo()
          errors.push('dialog')
        }
        resolve(accepted)
      })
    }),
    quietly: (op) => new Promise((resolve, reject) => {
      void op(client)
      answers.push((accepted) => (accepted ? resolve(undefined) : reject(new Error('refused'))))
    }),
  })
  return { reads, sent, errors, bot: () => bots.get('a')!, answer: (i: number, accepted: boolean) => answers[i]!(accepted), performed: () => answers.length }
}

const settle = () => new Promise((resolve) => setTimeout(resolve))

test('mark as unread is offered for a read chat on a host that has it', () => {
  const read = { unread: 0, keptUnread: false } as Bot
  assert.equal(offersMarkUnread(read, MARK_UNREAD_HOST), true)
  assert.equal(offersMarkUnread({ ...read, unread: 1 }, MARK_UNREAD_HOST), false)
  assert.equal(offersMarkUnread({ ...read, keptUnread: true }, MARK_UNREAD_HOST), false)
  assert.equal(offersMarkUnread(read, '2.33.0'), false)
  assert.equal(offersMarkUnread(read, undefined), false)
})

test('mark as unread shows what the host answers', async () => {
  const { reads, sent, bot, answer } = computer({}, (bots) => ({ bot: { ...bots.get('a'), rev: 5, unread: 2, keptUnread: true } }))
  reads.markUnread('a')
  await settle()
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }]])
  answer(0, true)
  await settle()
  assert.deepEqual([bot().rev, bot().unread, bot().keptUnread], [5, 2, true])
})

test('an answer older than the bot event that beat it is left out', async () => {
  const { reads, bot, answer } = computer({}, (bots) => ({ bot: { ...bots.get('a'), rev: 5, keptUnread: true } }))
  reads.markUnread('a')
  await settle()
  Object.assign(bot(), { rev: 6, keptUnread: false })
  answer(0, true)
  await settle()
  assert.deepEqual([bot().rev, bot().keptUnread], [6, false])
})

test('opening a chat reads it whatever is known here of it, so a mark set elsewhere and still on its way ends too', async () => {
  const { reads, sent } = computer({})
  reads.open('a')
  await settle()
  assert.deepEqual(sent, [['markRead', { botId: 'a' }]])
})

test('a queued mark starts its retry window only once the mark before it is done', async () => {
  const { reads, answer, performed } = computer({ unread: 2 })
  reads.markRead('a')
  reads.markUnread('a')
  await settle()
  assert.equal(performed(), 1)
  answer(0, true)
  await settle()
  assert.equal(performed(), 2)
})

test('opening a chat shows it read at once', () => {
  const { reads, bot } = computer({ unread: 2, keptUnread: true })
  reads.open('a')
  assert.deepEqual([bot().unread, bot().keptUnread], [0, false])
})

test('a chat that fails to open stays quiet, gives its count back and reads as opened the next time it is on screen', async () => {
  const { reads, sent, errors, bot, answer } = computer({ rev: 1, unread: 3, keptUnread: true })
  assert.equal(reads.retryOpen('a'), false, 'nothing to send again yet')
  reads.open('a')
  answer(0, false)
  await settle()
  assert.deepEqual([bot().unread, bot().keptUnread], [3, true])
  assert.deepEqual(errors, [])
  assert.equal(reads.retryOpen('a'), true)
  answer(1, true)
  await settle()
  assert.deepEqual(sent, [['markRead', { botId: 'a' }], ['markRead', { botId: 'a' }]])
  assert.deepEqual([bot().unread, bot().keptUnread], [0, false])
  assert.equal(reads.retryOpen('a'), false, 'it went')
})

test('an open that fails while a newer mark waits behind it does not go again', async () => {
  const { reads, answer } = computer({})
  reads.open('a')
  reads.markUnread('a')
  answer(0, false)
  await settle()
  assert.equal(reads.retryOpen('a'), false)
})

test('marking a chat unread drops an open that never reached the host', async () => {
  const { reads, answer } = computer({})
  reads.open('a')
  answer(0, false)
  await settle()
  reads.markUnread('a')
  assert.equal(reads.retryOpen('a'), false)
})

test('a refused mark as read gives the hold and the unread count back and reaches the dialog', async () => {
  const { reads, bot, errors, answer } = computer({ rev: 1, unread: 3, keptUnread: true })
  reads.markRead('a')
  assert.deepEqual([bot().unread, bot().keptUnread], [0, false])
  answer(0, false)
  await settle()
  assert.deepEqual([bot().unread, bot().keptUnread], [3, true])
  assert.deepEqual(errors, ['dialog'])
})

test('two reads refused in a row both give back what was there before the first', async () => {
  const { reads, bot, answer } = computer({ rev: 1, unread: 3, keptUnread: true })
  reads.open('a')
  reads.open('a')
  answer(0, false)
  await settle()
  answer(1, false)
  await settle()
  assert.deepEqual([bot().unread, bot().keptUnread], [3, true])
})

test('a read after a newer bot event gives back what that event showed', async () => {
  const { reads, bot, answer } = computer({ rev: 1, unread: 3, keptUnread: true })
  reads.open('a')
  Object.assign(bot(), { rev: 2, unread: 1, keptUnread: false })
  reads.open('a')
  answer(0, false)
  await settle()
  answer(1, false)
  await settle()
  assert.deepEqual([bot().unread, bot().keptUnread], [1, false])
})

test('a refused mark as read keeps what a newer bot event from the host shows', async () => {
  const { reads, bot, answer } = computer({ rev: 1, unread: 3, keptUnread: true })
  reads.markRead('a')
  Object.assign(bot(), { rev: 2, unread: 0, keptUnread: false })
  answer(0, false)
  await settle()
  assert.deepEqual([bot().unread, bot().keptUnread], [0, false])
})

test('mark as unread after a mark as read still retrying waits for it, so the read lands first', async () => {
  const { reads, sent, answer } = computer({ unread: 2 })
  reads.markRead('a')
  reads.markUnread('a')
  await settle()
  assert.deepEqual(sent, [['markRead', { botId: 'a' }]])
  answer(0, true)
  await settle()
  assert.deepEqual(sent, [['markRead', { botId: 'a' }], ['markUnread', { botId: 'a' }]])
})

test('opening a chat right after marking it unread waits for the mark, so the read lands last', async () => {
  const { reads, sent, answer } = computer({})
  reads.markUnread('a')
  reads.open('a')
  await settle()
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }]])
  answer(0, true)
  await settle()
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }], ['markRead', { botId: 'a' }]])
})
