import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
import type { HostClient } from '../client/host-client'
import { BotSections } from './bot-sections.ts'

/**
 * A computer's sections over bots in `placed` (id → section), with a host that answers each call only
 * when the test says: `answer(i, false)` refuses call `i` as `perform` does (undo, then settle).
 */
function computer(placed: Record<string, string>) {
  const bots = new Map(Object.entries(placed).map(([id, section]) => [id, { id, section } as Bot]))
  const sent: unknown[][] = []
  const answers: ((accepted: boolean) => void)[] = []
  const sections = new BotSections(() => bots, () => {}, (op, undo) => new Promise<boolean>((resolve) => {
    void op({ call: async (method: string, body: unknown) => sent.push([method, body]) } as unknown as HostClient)
    answers.push((accepted) => {
      if (!accepted) undo()
      resolve(accepted)
    })
  }))
  return {
    sections, sent,
    bot: (id: string) => bots.get(id)!,
    where: () => Object.fromEntries([...bots.values()].map((bot) => [bot.id, bot.section])),
    answer: (i: number, accepted: boolean) => answers[i]!(accepted),
  }
}

test('a move sends only the section and shows at once; a refusal puts it back', async () => {
  const { sections, sent, bot, where, answer } = computer({ a: 'Work' })
  const moved = sections.move(bot('a'), ' Home ')
  assert.deepEqual(sent, [['updateBot', { id: 'a', section: 'Home' }]])
  assert.deepEqual(where(), { a: 'Home' })
  answer(0, false)
  assert.equal(await moved, false)
  assert.deepEqual(where(), { a: 'Work' })
})

test('chained moves refused in order end where the host still has the bot', async () => {
  // With the link down: a goes Work → Home, then Home → Side; both are refused after the wait.
  const { sections, bot, where, answer } = computer({ a: 'Work', b: 'Home' })
  const first = sections.move(bot('a'), 'Home')
  const second = sections.move(bot('a'), 'Side')
  const other = sections.move(bot('b'), 'Side') // Another bot's move from Home is not chained.
  answer(0, false)
  await first
  assert.deepEqual(where(), { a: 'Side', b: 'Side' }) // Moved again since: left alone.
  answer(1, false)
  await second
  assert.deepEqual(where(), { a: 'Work', b: 'Side' }) // Not Home, which the host never had.
  answer(2, false)
  await other
  assert.deepEqual(where(), { a: 'Work', b: 'Home' })
})

test('chained moves refused out of order end where the host still has the bot', async () => {
  const { sections, bot, where, answer } = computer({ a: 'Work' })
  const moves = [sections.move(bot('a'), 'Home'), sections.move(bot('a'), 'Side'), sections.move(bot('a'), 'X')]
  for (const i of [1, 0, 2]) {
    answer(i, false)
    await moves[i]
  }
  assert.deepEqual(where(), { a: 'Work' })
})

test('a refused rename followed by a refused move of one of its bots puts both bots back', async () => {
  const { sections, bot, where, answer } = computer({ w1: 'Work', w2: 'Work', h: 'Home' })
  const renamed = sections.rename('Work', 'Home')
  const moved = sections.move(bot('w1'), 'Side')
  answer(0, false)
  await renamed
  assert.deepEqual(where(), { w1: 'Side', w2: 'Work', h: 'Home' })
  answer(1, false)
  await moved
  assert.deepEqual(where(), { w1: 'Work', w2: 'Work', h: 'Home' })
})

test('a refused rename runs its callback while its bots are still moved, then puts them back; an accepted one never does', async () => {
  const { sections, where, answer } = computer({ w: 'Work' })
  const seen: Record<string, string | undefined>[] = []
  const refused = sections.rename('Work', 'Jobs', () => seen.push(where()))
  answer(0, false)
  await refused
  assert.deepEqual(where(), { w: 'Work' })
  const accepted = sections.rename('Work', 'Jobs', () => seen.push(where()))
  answer(1, true)
  await accepted
  assert.deepEqual(seen, [{ w: 'Jobs' }])
})

test('an accepted move stays, and later refusals no longer chain to it', async () => {
  const { sections, bot, where, answer } = computer({ a: 'Work' })
  const first = sections.move(bot('a'), 'Home')
  answer(0, true)
  assert.equal(await first, true)
  const second = sections.move(bot('a'), 'Side')
  answer(1, false)
  await second
  assert.deepEqual(where(), { a: 'Home' })
})
