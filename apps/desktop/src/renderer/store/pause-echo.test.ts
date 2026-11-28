// The roster row shows "Paused" only when the stored bot is paused (BotRow.tsx Preview).
// An older host can accept updateBot and omit `paused`. That success must not leave the row saying Paused.
import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { Bot } from '@shared/models'
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

const storage = new Map<string, string>()
Object.assign(globalThis, {
  localStorage: {
    getItem: (k: string) => storage.get(k) ?? null,
    setItem: (k: string, v: string) => void storage.set(k, v),
    removeItem: (k: string) => void storage.delete(k),
  },
  window: { addEventListener() {}, removeEventListener() {}, codync: { appVersion: '2.51.0' } },
})

const { normalizeBot, isGroup } = await import('@shared/models')
const { HostClient } = await import('../client/host-client.ts')
const { BotStore } = await import('./bot-store.ts')

/** What the roster row renders as the word Paused: BotRow.tsx shows that line only for a paused bot that is not a group. */
const rowSaysPaused = (bot: Bot) => bot.paused && !isGroup(bot)

/** A store already online, whose `updateBot` answers with `answer` once the call is in flight. */
function onlineStore(answer: unknown) {
  let sawCall: () => void = () => {}
  const called = new Promise<void>((resolve) => { sawCall = resolve })
  const sent: [string, unknown][] = []
  const transport: HostTransport = {
    call: async (method, body) => {
      sent.push([method, body])
      sawCall()
      return answer
    },
    stream: () => () => {},
  }
  const store = new BotStore({ id: 'c1', name: 'Mac', signKey: '', urls: [] }, true, () => transport, '2.51.0')
  const bot = normalizeBot({ id: 'b', name: 'Ada', paused: false })
  store.bots.set(bot.id, bot)
  store.connection = { kind: 'online' }
  store.client = new HostClient(transport, '2.50.0')
  return { store, bot, sent, called }
}

/** Lets the pause call's echo land. The menu does not await `setPaused`, so the test waits for the host answer and the microtasks after it. */
async function settle(called: Promise<void>) {
  await called
  await new Promise((resolve) => setImmediate(resolve))
}

test("an older host that accepts updateBot and omits paused does not leave the roster showing Paused", async () => {
  const { store, bot, sent, called } = onlineStore({ bot: { id: 'b', name: 'Ada' } })
  store.setPaused(bot, true)
  await settle(called)
  const shown = store.bots.get('b')!
  assert.equal(sent[0]?.[0], 'updateBot')
  assert.equal((sent[0]?.[1] as { paused?: boolean }).paused, true)
  assert.equal(rowSaysPaused(shown), false)
  assert.equal(shown.paused, false)
  assert.equal(store.lastError, 'Not applied on this computer')
})

test('a host that echoes paused true leaves the roster showing Paused and no error', async () => {
  const { store, bot, called } = onlineStore({ bot: { id: 'b', name: 'Ada', paused: true } })
  store.setPaused(bot, true)
  await settle(called)
  const shown = store.bots.get('b')!
  assert.equal(rowSaysPaused(shown), true)
  assert.equal(store.lastError, null)
})

test('a host that echoes paused as a string does not leave the roster showing Paused', async () => {
  const { store, bot, called } = onlineStore({ bot: { id: 'b', name: 'Ada', paused: 'yes' } })
  store.setPaused(bot, true)
  await settle(called)
  const shown = store.bots.get('b')!
  assert.equal(rowSaysPaused(shown), false)
  assert.equal(shown.paused, false)
  assert.equal(store.lastError, 'Not applied on this computer')
})

test('a host that echoes paused false after a pause request does not leave the roster showing Paused', async () => {
  const { store, bot, called } = onlineStore({ bot: { id: 'b', name: 'Ada', paused: false } })
  store.setPaused(bot, true)
  await settle(called)
  const shown = store.bots.get('b')!
  assert.equal(rowSaysPaused(shown), false)
  assert.equal(store.lastError, 'Not applied on this computer')
})
