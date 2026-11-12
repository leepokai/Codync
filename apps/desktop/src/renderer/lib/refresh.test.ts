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

test("another device's auto order switch shows here", async () => {
  let send!: (event: object) => void
  const ready = deferred()
  const store = sync({
    call: async () => ({ version: '2.29.0', name: 'Test', backends: [], autoOrder: false }),
    stream: (_path, onData) => { send = (event) => onData(JSON.stringify(event)); ready.resolve(); return () => {} },
  })
  store.start()
  await ready.promise
  assert.equal(store.hello?.autoOrder, false)
  send({ type: 'autoOrder', enabled: true })
  assert.equal(store.hello?.autoOrder, true)
  store.retire()
})

/** A store over the cache another app version left: "gone" and "kept" from the computer and a message that didn't send, with each events stream it opens. */
function syncOverStaleCache() {
  const entry = (id: string, seq: number, rev: number, data = {}) => ({ id, seq, botId: 'b1', rev, kind: 'agent', turn: 1, data, createdAt: 1, updatedAt: 1 })
  storage.set('codync-mirror-local-test', JSON.stringify({ stamp: 'older/2.0.0', hostId: 'h1', rev: 77, bots: [],
    entries: [entry('gone', 1, 70), entry('kept', 2, 77), entry('local-x', 3, 0, { status: 'failed' })] }))
  const streams: { path: string; send: (event: object) => void }[] = []
  let opened = deferred()
  const store = sync({
    call: async () => ({ version: '2.29.0', name: 'Test', backends: [] }),
    stream: (path, onData) => { streams.push({ path, send: (event) => onData(JSON.stringify(event)) }); opened.resolve(); return () => {} },
  })
  const nextStream = async () => { await opened.promise; opened = deferred(); return streams[streams.length - 1] }
  return { store, nextStream, kept: { type: 'entry', entry: entry('kept', 2, 77) }, ids: () => store.allEntries('b1').map((e) => e.id) }
}

test('a new version refetches underneath the cached chat', async () => {
  const { store, nextStream, kept, ids } = syncOverStaleCache()
  store.start()
  const stream = await nextStream()
  assert.match(stream.path, /since=0&/)
  stream.send({ type: 'hello', hostId: 'h1', rev: 80, catchUp: 3 })
  stream.send({ type: 'bot', bot: { id: 'b1', name: 'Bot', rev: 1 } })
  stream.send(kept)
  assert.deepEqual(ids(), ['gone', 'kept', 'local-x'], 'the chat keeps what it showed until the catch-up has all arrived')
  assert.equal(store.canLoadOlder('b1'), false)
  stream.send({ type: 'somethingNew' })
  assert.deepEqual(ids(), ['kept', 'local-x'], 'an event this app has no use for counts too')
  assert.equal(store.canLoadOlder('b1'), true)
  store.retire()
})

test('a refetch the link cut short starts over', async () => {
  const { store, nextStream, kept, ids } = syncOverStaleCache()
  const savedStamp = () => { store.saveCache(); return JSON.parse(storage.get('codync-mirror-local-test')!).stamp }
  store.start()
  const first = await nextStream()
  first.send({ type: 'hello', hostId: 'h1', rev: 80, catchUp: 3 })
  first.send(kept)
  assert.equal(savedStamp(), '')
  store.restartStream()
  const second = await nextStream()
  assert.match(second.path, /since=0&/)
  assert.deepEqual(ids(), ['gone', 'kept', 'local-x'])
  second.send({ type: 'hello', hostId: 'h1', rev: 80 })
  assert.deepEqual(ids(), ['local-x'], "a host that doesn't say how long its catch-up is: the held entries go at its hello")
  assert.equal(savedStamp(), '2.29.0/2.29.0')
  store.retire()
})

test("an unreadable event fetches everything again; the stream it ended doesn't finish that refetch", async () => {
  const { store, nextStream, ids } = syncOverStaleCache()
  store.start()
  const first = await nextStream()
  first.send({ type: 'hello', hostId: 'h1', rev: 80, catchUp: 1 })
  first.send({ type: 'entry' })
  await nextStream()
  assert.deepEqual(ids(), ['gone', 'kept', 'local-x'])
  store.retire()
})

test('a pairing answer the host never got brings the question back', async () => {
  const app = new AppModel(false)
  const store = { isLoopback: true, computer: { id: 'mac' }, pairingRequests: [{ requestId: 'r1' }],
    ready: async () => ({ call: () => Promise.reject(new Error('Offline')) }) } as unknown as BotStore
  app.stores.set('mac', store)
  const [ask] = app.pairingAsks
  const answer = app.decidePairing(ask, true)
  assert.equal(app.pairingAsks.length, 0, 'off the screen at once')
  assert.equal(await answer, false)
  assert.equal(app.lastError, 'Offline')
  assert.equal(app.pairingAsks.length, 1, 'the question comes back')
  app.stores.clear()
})
