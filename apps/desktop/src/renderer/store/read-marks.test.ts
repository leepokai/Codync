import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { ReadMarks } from './read-marks.ts'

function computer(bot: Partial<Bot>) {
  const bots = new Map([['a', { id: 'a', unread: 0, keptUnread: false, ...bot } as Bot]])
  const sent: unknown[][] = []
  const answers: ((accepted: boolean) => void)[] = []
  const reads = new ReadMarks(() => bots, () => {}, (op, undo) => new Promise<boolean>((resolve) => {
    void op({ call: async (method: string, body: unknown) => sent.push([method, body]) } as unknown as HostClient)
    answers.push((accepted) => {
      if (!accepted) undo()
      resolve(accepted)
    })
  }))
  return { reads, sent, bot: () => bots.get('a')!, answer: (i: number, accepted: boolean) => answers[i]!(accepted) }
}

test('mark as unread holds the chat unread at once', () => {
  const { reads, sent, bot } = computer({})
  reads.markUnread('a')
  assert.deepEqual(sent, [['markUnread', { botId: 'a' }]])
  assert.equal(bot().keptUnread, true)
})

test('a refused mark as unread lets the chat be read again', async () => {
  const { reads, bot, answer } = computer({})
  reads.markUnread('a')
  answer(0, false)
  await Promise.resolve()
  assert.equal(bot().keptUnread, false)
})

test('opening a chat marked unread reads it', () => {
  const { reads, sent, bot } = computer({ unread: 2, keptUnread: true })
  reads.open('a')
  assert.deepEqual(sent, [['markRead', { botId: 'a' }]])
  assert.deepEqual([bot().unread, bot().keptUnread], [0, false])
})

test('opening a chat not marked unread leaves reading to the view', () => {
  const { reads, sent } = computer({ unread: 2 })
  reads.open('a')
  assert.deepEqual(sent, [])
})

test('mark as read clears a hold even with nothing counted unread', () => {
  const { reads, sent, bot } = computer({ keptUnread: true })
  reads.markRead('a')
  assert.deepEqual(sent, [['markRead', { botId: 'a' }]])
  assert.equal(bot().keptUnread, false)
})
