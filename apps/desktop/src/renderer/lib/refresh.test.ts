import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { BotStore } from '../store/bot-store'
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
  localStorage: { getItem: (key: string) => storage.get(key) ?? null, setItem: (key: string, value: string) => storage.set(key, value), removeItem: (key: string) => storage.delete(key) },
  window: { addEventListener() {}, codync: { appVersion: '2.29.0' } },
})
const { AppModel } = await import('../store/app-model.ts')
const { BotSync } = await import('../store/bot-sync.ts')

function deferred() {
  let resolve!: () => void
  const promise = new Promise<void>((r) => { resolve = r })
  return { promise, resolve }
}

test('refresh is visible immediately, ignores repeat shortcuts, and waits for computers and cloud', async () => {
  const app = new AppModel(false)
  const computer = deferred()
  const cloud = deferred()
  let calls = 0
  app.stores.set('test', { refreshStream: () => { calls++; return computer.promise } } as unknown as BotStore)
  app.cloud.refresh = async () => { await cloud.promise; return true }
  const refreshing = app.refresh()
  assert.equal(app.refreshing, true)
  await app.refresh()
  assert.equal(calls, 1)
  computer.resolve()
  await new Promise((r) => setTimeout(r, 650))
  assert.equal(app.refreshing, true)
  cloud.resolve()
  await refreshing
  assert.equal(app.refreshing, false)
})

test('a failed refresh clears loading, reports the failure and can be retried', async () => {
  const app = new AppModel(false)
  app.stores.set('test', { refreshStream: () => Promise.reject(new Error('Offline')) } as unknown as BotStore)
  await app.refresh()
  assert.equal(app.refreshing, false)
  assert.equal(app.lastError, 'Offline')
  app.stores.clear()
  app.clearErrors()
  await app.refresh()
  assert.equal(app.refreshing, false)
  assert.equal(app.lastError, null)
})

function sync(transport: HostTransport) {
  return new BotSync({ id: 'test', name: 'Test', signKey: '', urls: [] }, false, () => transport)
}

test('an already-online computer stays refreshing until the replacement stream answers', async () => {
  let receive!: (line: string) => void
  const ready = deferred()
  const store = sync({
    call: async () => ({ version: '2.29.0', name: 'Test', backends: [] }),
    stream: (_path, onData) => { receive = onData; ready.resolve(); return () => {} },
  })
  store.connection = { kind: 'online' }
  let completed = false
  const refresh = store.refreshStream().then(() => { completed = true })
  await ready.promise
  assert.equal(completed, false)
  receive(JSON.stringify({ type: 'usage', usage: { providers: [] } }))
  await refresh
  assert.equal(completed, true)
  store.retire()
})

test('a host failure and a removed computer both release the refresh waiter', async () => {
  const failing = sync({ call: async () => { throw new Error('Unavailable') }, stream: () => () => {} })
  await assert.rejects(failing.refreshStream(), /Unavailable/)
  failing.retire()
  const removed = sync({ call: () => new Promise(() => {}), stream: () => () => {} })
  const refresh = removed.refreshStream()
  removed.retire()
  await refresh
})

test('an unresponsive host cannot leave manual refresh spinning forever', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const store = sync({ call: () => new Promise(() => {}), stream: () => () => {} })
  const refresh = store.refreshStream()
  const rejected = assert.rejects(refresh, /Connection timed out/)
  t.mock.timers.tick(20_000)
  await rejected
  store.retire()
})
