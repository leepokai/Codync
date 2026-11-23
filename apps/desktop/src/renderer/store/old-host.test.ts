// Hosts before 2.51.0: goes when MIN_HOST reaches 2.51.0.
import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { Bot, Entry } from '@shared/models'
import type { HostTransport } from '../client/host-client'

registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('@shared/')) return nextResolve(new URL(`../../shared/${specifier.slice(8)}.ts`, import.meta.url).href, context)
  if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
    const url = new URL(`${specifier}.ts`, context.parentURL)
    if (existsSync(url)) return nextResolve(url.href, context)
  }
  return nextResolve(specifier, context)
}, load(url, context, nextLoad) {
  if (url.endsWith('.ts')) return { format: 'module', shortCircuit: true, source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText }
  return nextLoad(url, context)
} })

const { HostClient, parseEvent } = await import('../client/host-client.ts')
const { speaksThreads } = await import('@shared/old-host')

/** A client of a host at `version` that answers each method with `answers[method]`, and what it was sent. */
async function clientOf(version: string, answers: Record<string, unknown> = {}) {
  const sent: [string, unknown][] = []
  const transport: HostTransport = {
    call: async (method, body) => {
      if (method !== 'hello') sent.push([method, body])
      return method === 'hello' ? { version } : answers[method] ?? {}
    },
    stream: () => () => {},
  }
  const client = new HostClient(transport)
  await client.hello()
  return { client, sent }
}

/** Every call that names a task or a thread, in the names of today. */
async function callAll(client: InstanceType<typeof HostClient>) {
  await client.call('send', { botId: 'b', taskId: 't1', text: 'hi', clientNonce: 'n' })
  await client.call('steer', { botId: 'b', taskId: 't1', entryId: 'e' })
  await client.call('unqueue', { botId: 'b', taskId: 't1', entryId: 'e' })
  await client.call('workFirst', { botId: 'b', taskId: 't1' })
  await client.call('workCutIn', { botId: 'b', taskId: 't1', insteadOf: 't2' })
  await client.call('dropWork', { botId: 'b', taskId: 't1' })
  await client.call('stopWork', { botId: 'b', taskId: 't1' })
  await client.call('stopWork', { botId: 'b' })
  await client.call('retryWork', { botId: 'b', taskId: 't1' })
  await client.call('threadEntries', { botId: 'b', threadId: 'task-1' })
  await client.call('updateBot', { id: 'b', maxThreads: 2, threadWorktrees: false })
  await client.call('send', { botId: 'b', text: 'to the chat' })
}

test('the first host that says task and thread is 2.51.0; one not known yet counts as new', () => {
  assert.equal(speaksThreads('2.51.0'), true)
  assert.equal(speaksThreads('2.52.3'), true)
  assert.equal(speaksThreads('2.50.9'), false)
  assert.equal(speaksThreads('2.26.0'), false)
  assert.equal(speaksThreads(undefined), true)
})

/** What `callAll` sends, with a thread's transcript asked for as `transcript`. */
const sentWith = (transcript: [string, unknown]) => [
  ['send', { botId: 'b', taskId: 't1', itemId: 't1', text: 'hi', clientNonce: 'n' }],
  ['steer', { botId: 'b', taskId: 't1', itemId: 't1', entryId: 'e' }],
  ['unqueue', { botId: 'b', taskId: 't1', itemId: 't1', entryId: 'e' }],
  ['workFirst', { botId: 'b', taskId: 't1', itemId: 't1' }],
  ['workCutIn', { botId: 'b', taskId: 't1', itemId: 't1', insteadOf: 't2' }],
  ['dropWork', { botId: 'b', taskId: 't1', itemId: 't1' }],
  ['stopWork', { botId: 'b', taskId: 't1', itemId: 't1' }],
  ['stopWork', { botId: 'b' }],
  ['retryWork', { botId: 'b', taskId: 't1', itemId: 't1' }],
  transcript,
  ['updateBot', { id: 'b', maxThreads: 2, threadWorktrees: false, maxAgents: 2, taskWorktrees: false }],
  ['send', { botId: 'b', text: 'to the chat' }],
]

test('a task and a bot\'s thread settings go under both names; an older host is asked for taskEntries', async () => {
  const { client, sent } = await clientOf('2.50.0')
  await callAll(client)
  assert.deepEqual(sent, sentWith(['taskEntries', { botId: 'b', taskId: 'task-1' }]))
})

test('a host from 2.51.0 is asked for threadEntries', async () => {
  const { client, sent } = await clientOf('2.51.0')
  await callAll(client)
  assert.deepEqual(sent, sentWith(['threadEntries', { botId: 'b', threadId: 'task-1' }]))
})

test('before hello, a call to a task still reaches it on either host; a new connection starts from the known version', async () => {
  const sent: [string, unknown][] = []
  const transport: HostTransport = { call: async (method, body) => { sent.push([method, body]); return {} }, stream: () => () => {} }
  await new HostClient(transport).call('stopWork', { botId: 'b', taskId: 't1' })
  await new HostClient(transport, '2.50.1').call('threadEntries', { botId: 'b', threadId: 'task-1' })
  assert.deepEqual(sent, [
    ['stopWork', { botId: 'b', taskId: 't1', itemId: 't1' }],
    ['taskEntries', { botId: 'b', taskId: 'task-1' }],
  ])
})

const oldCard = { id: 'e1', seq: 1, botId: 'b', rev: 1, kind: 'permission', turn: 1, data: { status: 'pending', title: 'npm test', item: 'i1', task: { id: 'task-1', title: 'Run the suite' } } }
const oldBot = { id: 'b', name: 'Bot', maxAgents: 2, taskWorktrees: false, work: [{ id: 'i1', title: 'Audit', status: 'working', task: 'task-1' }, { id: 'i2', title: 'Next', status: 'waiting', task: null }] }

test("an older host's events read as today's: its bot, its card and its taskEntry", () => {
  const bot = parseEvent(JSON.stringify({ type: 'bot', rev: 3, bot: oldBot })) as { type: 'bot'; bot: Bot }
  assert.deepEqual([bot.bot.maxThreads, bot.bot.threadWorktrees], [2, false])
  assert.deepEqual(bot.bot.work.map((t) => [t.id, t.thread]), [['i1', 'task-1'], ['i2', null]])

  const card = parseEvent(JSON.stringify({ type: 'entry', rev: 4, entry: oldCard })) as { type: 'entry'; entry: Entry }
  assert.deepEqual([card.entry.data.task, card.entry.data.thread], ['i1', { id: 'task-1', title: 'Run the suite' }])
  assert.equal('item' in card.entry.data, false)

  const step = { id: 's1', seq: 1, botId: 'task-1', rev: 5, kind: 'tool', turn: 1, data: { title: 'ls' } }
  assert.deepEqual(parseEvent(JSON.stringify({ type: 'taskEntry', rev: 5, task: 'task-1', entry: step })), { type: 'threadEntry', thread: 'task-1', entry: step })
})

test("today's events read the same, whatever the host's version", () => {
  const card = { ...oldCard, data: { status: 'pending', title: 'npm test', task: 'i1', thread: { id: 'thread-1', title: 'Run the suite' } } }
  assert.deepEqual((parseEvent(JSON.stringify({ type: 'entry', rev: 4, entry: card })) as { entry: Entry }).entry.data, card.data)
  const bot = parseEvent(JSON.stringify({ type: 'bot', rev: 3, bot: { id: 'b', maxThreads: 4, threadWorktrees: true, work: [{ id: 't1', status: 'working', thread: 'thread-1' }] } })) as { bot: Bot }
  assert.deepEqual([bot.bot.maxThreads, bot.bot.threadWorktrees, bot.bot.work[0]!.thread], [4, true, 'thread-1'])
})

test("an older host's answers read as today's: the bot of updateBot, a page of history, a thread's transcript", async () => {
  const { client } = await clientOf('2.50.0', {
    updateBot: { bot: oldBot },
    history: { entries: [oldCard] },
    taskEntries: { task: { id: 'task-1', title: 'Audit' }, entries: [] },
  })
  const { bot } = await client.call<{ bot: { maxThreads: number; work: { thread: string | null }[] } }>('updateBot', { id: 'b' })
  assert.deepEqual([bot.maxThreads, bot.work[0]!.thread], [2, 'task-1'])
  const [card] = await client.history('b', 10, 100)
  assert.deepEqual([card!.data.task, card!.data.thread?.id], ['i1', 'task-1'])
  assert.deepEqual(await client.call('threadEntries', { botId: 'b', threadId: 'task-1' }), { thread: { id: 'task-1', title: 'Audit' }, entries: [] })
})
