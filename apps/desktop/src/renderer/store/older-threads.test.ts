import assert from 'node:assert/strict'
import { registerHooks } from 'node:module'
import { test } from 'node:test'

registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('@shared/')) return nextResolve(new URL(`../../shared/${specifier.slice(8)}.ts`, import.meta.url).href, context)
  return nextResolve(specifier, context)
} })

const { OlderThreads, keepChoices, parseOlderThreads, withOlderThreads } = await import('./older-threads.ts')

const row = (botId: string, id: string, endedAt: number) => ({ id, botId, title: id, status: 'done', thread: `t-${id}`, createdAt: endedAt - 10, endedAt })

test('a page of older threads reads as work rows with their bot, skipping ones without a bot', () => {
  const page = parseOlderThreads({ threads: [row('b1', 'a', 100), { id: 'x', status: 'done' }], more: true })
  assert.equal(page.more, true)
  assert.deepEqual(page.threads.map((t) => [t.botId, t.id, t.status, t.endedAt]), [['b1', 'a', 'done', 100]])
  assert.deepEqual(parseOlderThreads(null), { threads: [], more: false })
})

test("older threads join their bot's work, and a task its work has keeps the bot's row", () => {
  const live = { id: 'a', title: 'live' }
  const roster = [{ bot: { id: 'b1', work: [live] } }, { bot: { id: 'b2', work: [] } }] as unknown as Parameters<typeof withOlderThreads>[0]
  const older = parseOlderThreads({ threads: [row('b1', 'a', 100), row('b1', 'b', 90), row('b2', 'c', 80), row('gone', 'd', 70)] }).threads
  const merged = withOlderThreads(roster, () => older)
  assert.deepEqual(merged[0]!.bot.work.map((t) => [t.id, t.title]), [['a', 'live'], ['b', 'b']])
  assert.deepEqual(merged[1]!.bot.work.map((t) => t.id), ['c'])
})

test('the next page asks before the oldest row, drops repeats and stops when the host has no more', async () => {
  const list = new OlderThreads(() => {})
  const asked: (number | undefined)[] = []
  const pages = [{ threads: [row('b1', 'a', 300), row('b1', 'b', 200)], more: true }, { threads: [row('b1', 'b', 200), row('b2', 'c', 100)], more: false }]
  const ask = async (before: number | undefined) => (asked.push(before), pages.shift())
  await list.loadNext(ask)
  await list.loadNext(ask)
  await list.loadNext(ask)
  assert.deepEqual(asked, [undefined, 200])
  assert.deepEqual(list.rows.map((t) => t.id), ['a', 'b', 'c'])
  list.reset()
  assert.deepEqual([list.rows.length, list.more, list.started], [0, true, false])
})

test('a failed page or one that adds nothing stops paging; Drop takes a row out and can put it back', async () => {
  const failing = new OlderThreads(() => {})
  await assert.rejects(failing.loadNext(async () => { throw new Error('offline') }))
  let asked = 0
  await failing.loadNext(async () => (asked++, { threads: [], more: true }))
  assert.equal(asked, 0)

  const list = new OlderThreads(() => {})
  await list.loadNext(async () => ({ threads: [row('b1', 'a', 300), row('b1', 'b', 200)], more: true }))
  await list.loadNext(async () => ({ threads: [row('b1', 'b', 200)], more: true }))
  assert.equal(list.more, false)
  const undo = list.drop('b1', 'a')
  assert.deepEqual(list.rows.map((t) => t.id), ['b'])
  undo()
  assert.deepEqual(list.rows.map((t) => t.id), ['a', 'b'])
})

test("a thread that leaves its bot's work stays listed as it last read, without moving the next page's cursor", async () => {
  const list = new OlderThreads(() => {})
  await list.loadNext(async () => ({ threads: [row('b1', 'a', 300), row('b1', 'old', 100)], more: true }))
  const fresh = { ...row('b1', 'a', 900), title: 'reopened' }
  list.keep('b1', [fresh, row('b1', 'live', 950)] as never, [{ id: 'live' }] as never)
  assert.deepEqual(list.shown.map((t) => [t.id, t.title]), [['a', 'reopened'], ['old', 'old']])
  const asked: (number | undefined)[] = []
  await list.loadNext(async (before) => (asked.push(before), { threads: [], more: false }))
  assert.deepEqual(asked, [100])
})

test('a dropped thread that leaves its bot\'s work is not kept, unless the drop is undone', () => {
  const list = new OlderThreads(() => {})
  const undo = list.drop('b1', 'a')
  list.keep('b1', [row('b1', 'a', 900)] as never, [])
  assert.equal(list.shown.length, 0)
  undo()
  list.keep('b1', [row('b1', 'a', 900)] as never, [])
  assert.deepEqual(list.shown.map((t) => t.id), ['a'])
})

test("Delete old threads offers never, a week, a month, three months, and the computer's own number in order", () => {
  assert.deepEqual(keepChoices(0), [0, 7, 30, 90])
  assert.deepEqual(keepChoices(14), [0, 7, 14, 30, 90])
})
