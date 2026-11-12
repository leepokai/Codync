import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { Bot } from '@shared/models'

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

const { FakeHost } = await import('./fake-host.ts')
const { macBook } = await import('./story.ts')

test('sending from a chat marked unread reads it, like the real host', async () => {
  const host = new FakeHost('2.47.0', macBook)
  await host.call('markRead', { botId: 'scout' })
  const { bot: held } = (await host.call('markUnread', { botId: 'scout' })) as { bot: Bot }
  assert.equal(held.keptUnread, true)

  const bots: Bot[] = []
  const stop = host.subscribe(Number.MAX_SAFE_INTEGER, (line) => {
    const event = JSON.parse(line) as { type: string; bot?: Bot }
    if (event.type === 'bot' && event.bot?.id === 'scout') bots.push(event.bot)
  })
  await new Promise((resolve) => setTimeout(resolve, 0))
  await host.call('send', { botId: 'scout', text: 'hi', clientNonce: 'n1' })
  await host.call('stop', { botId: 'scout' })
  stop()

  const last = bots.at(-1)
  assert.equal(last?.unread, 0)
  assert.equal(last?.keptUnread, false)
})

test('marking an empty chat unread shows no badge, like the real host', async () => {
  const host = new FakeHost('2.47.0', macBook)
  const { bot: created } = (await host.call('createBot', { name: 'Empty', kind: 'agent' })) as { bot: Bot }
  const { bot } = (await host.call('markUnread', { botId: created.id })) as { bot: Bot }
  assert.equal(bot.unread, 0)
  assert.equal(bot.keptUnread ?? false, false)
})

test('marking a read chat unread brings back how many replies were unread, like the real host', async () => {
  const host = new FakeHost('2.47.0', macBook)
  const reply = (host as unknown as { add(e: { botId: string; kind: string; data: object }): void }).add.bind(host)
  reply({ botId: 'scout', kind: 'agent', data: { text: 'one', final: true } })
  reply({ botId: 'scout', kind: 'agent', data: { text: 'two', final: true } })
  await host.call('markRead', { botId: 'scout' })
  const { bot } = (await host.call('markUnread', { botId: 'scout' })) as { bot: Bot }
  assert.equal(bot.unread, 3)
  assert.equal(bot.keptUnread, true)
})

test('marking a chat unread after a new reply keeps its count and holds it, like the real host', async () => {
  const host = new FakeHost('2.47.0', macBook)
  const reply = (host as unknown as { add(e: { botId: string; kind: string; data: object }): void }).add.bind(host)
  await host.call('markRead', { botId: 'scout' })
  reply({ botId: 'scout', kind: 'agent', data: { text: 'one', final: true } })
  reply({ botId: 'scout', kind: 'agent', data: { text: 'two', final: true } })
  const { bot } = (await host.call('markUnread', { botId: 'scout' })) as { bot: Bot }
  assert.equal(bot.unread, 2)
  assert.equal(bot.keptUnread, true)
  await host.call('markRead', { botId: 'scout', automatic: true })
  const { bot: still } = (await host.call('markUnread', { botId: 'scout' })) as { bot: Bot }
  assert.equal(still.unread, 2)
})
