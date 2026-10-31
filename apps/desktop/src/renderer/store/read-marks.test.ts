import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { ReadMarks } from './read-marks.ts'

function computer(bot: Partial<Bot>, answer: (bots: Map<string, Bot>) => unknown = () => ({})) {
  const bots = new Map([['a', { id: 'a', unread: 0, keptUnread: false, ...bot } as Bot]])
  const sent: unknown[][] = []
  const answers: ((accepted: boolean) => void)[] = []
  const reads = new ReadMarks(() => bots, () => {}, (op, undo) => new Promise<boolean>((resolve) => {
    void op({ call: async (method: string, body: unknown) => {
      sent.push([method, body])
      return answer(bots)
    } } as unknown as HostClient)
    answers.push((accepted) => {
      if (!accepted) undo()
      resolve(accepted)
    })
  }))
  return { reads, sent, bot: () => bots.get('a')!, answer: (i: number, accepted: boolean) => answers[i]!(accepted) }
}

test('mark as unread holds the chat at once, through a bot event from before it', () => {
  const { reads, sent, bot } = computer({})
  reads.markUnread('a')
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }]])
  reads.received(bot())
  assert.equal(reads.held('a'), true)
})

test('a refused mark as unread lets the chat be read again', async () => {
  const { reads, answer } = computer({})
  reads.markUnread('a')
  answer(0, false)
  await new Promise((resolve) => setTimeout(resolve))
  assert.equal(reads.held('a'), false)
})

test('the host showing the hold carries it from then on', () => {
  const { reads, bot } = computer({})
  reads.markUnread('a')
  reads.received({ ...bot(), keptUnread: true })
  assert.equal(reads.held('a'), false)
})

test('opening a chat marked unread reads it, after the mark goes through', async () => {
  const { reads, sent, bot, answer } = computer({})
  reads.markUnread('a')
  reads.open('a')
  assert.equal(reads.held('a'), false)
  await new Promise((resolve) => setTimeout(resolve))
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }]])
  answer(0, true)
  await new Promise((resolve) => setTimeout(resolve))
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }], ['markRead', { botId: 'a' }]])
  assert.deepEqual([bot().unread, bot().keptUnread], [0, false])
})

test('opening a chat not marked unread leaves reading to the view', () => {
  const { reads, sent } = computer({ unread: 2 })
  reads.open('a')
  assert.deepEqual(sent, [])
})

test('mark as read clears a hold even with nothing counted unread', async () => {
  const { reads, sent, bot } = computer({ keptUnread: true })
  reads.markRead('a')
  await new Promise((resolve) => setTimeout(resolve))
  assert.deepEqual(sent, [['markRead', { botId: 'a' }]])
  assert.equal(bot().keptUnread, false)
})

test('a mark as unread the host had nothing to hold for lets the chat be read again', async () => {
  const { reads, answer } = computer({}, (bots) => ({ bot: bots.get('a') }))
  reads.markUnread('a')
  await new Promise((resolve) => setTimeout(resolve))
  answer(0, true)
  await new Promise((resolve) => setTimeout(resolve))
  assert.equal(reads.held('a'), false)
})

test('a mark as unread the host holds keeps the hold until its event', async () => {
  const { reads, answer } = computer({}, (bots) => ({ bot: { ...bots.get('a'), keptUnread: true } }))
  reads.markUnread('a')
  await new Promise((resolve) => setTimeout(resolve))
  answer(0, true)
  await new Promise((resolve) => setTimeout(resolve))
  assert.equal(reads.held('a'), true)
})

test('a hold the host already let go is dropped by the next bot event after the mark', async () => {
  const { reads, bot, answer } = computer({}, (bots) => ({ bot: { ...bots.get('a'), rev: 5, keptUnread: true } }))
  reads.markUnread('a')
  await new Promise((resolve) => setTimeout(resolve))
  answer(0, true)
  await new Promise((resolve) => setTimeout(resolve))
  reads.received({ ...bot(), rev: 4 })
  assert.equal(reads.held('a'), true, 'an event from before the mark')
  reads.received({ ...bot(), rev: 6, keptUnread: false })
  assert.equal(reads.held('a'), false)
})

test('a refused mark as read gives the hold back', async () => {
  const { reads, bot, answer } = computer({ keptUnread: true })
  reads.markRead('a')
  assert.equal(bot().keptUnread, false)
  answer(0, false)
  await new Promise((resolve) => setTimeout(resolve))
  assert.equal(bot().keptUnread, true)
})
