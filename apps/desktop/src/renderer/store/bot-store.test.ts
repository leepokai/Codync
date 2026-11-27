import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { HostClient } from '../client/host-client'

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
/** Resolves on the next microtask, after the caller has assigned `onsuccess`. */
function doneRequest(result?: unknown) {
  const request = { result, onsuccess: null as null | (() => void), onerror: null as null | (() => void), onupgradeneeded: null as null | (() => void) }
  queueMicrotask(() => request.onsuccess?.())
  return request
}
Object.assign(globalThis, {
  localStorage: { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => void storage.set(k, v), removeItem: (k: string) => void storage.delete(k) },
  window: { addEventListener() {}, removeEventListener() {}, codync: { appVersion: '2.54.0' } },
  indexedDB: { open: () => doneRequest({ close() {}, createObjectStore() {}, transaction: () => ({ objectStore: () => ({ get: () => doneRequest(undefined) }) }) }) },
})

const { BotStore } = await import('./bot-store.ts')
const { normalizeBot } = await import('../../shared/models.ts')

/** A promise the test settles after the roster has moved on. */
function gate<T>() {
  let resolve: (value: T) => void = () => {}
  let reject: (error: unknown) => void = () => {}
  const promise = new Promise<T>((res, rej) => { resolve = res; reject = rej })
  return { promise, resolve, reject }
}

function ticks(n = 8) {
  let chain: Promise<void> = Promise.resolve()
  for (let i = 0; i < n; i++) chain = chain.then(() => new Promise((r) => setImmediate(r)))
  return chain
}

/** Online store whose `updateBot` waits on `update`; every other call succeeds. */
function pausingStore() {
  const update = gate<{ bot?: { id?: string; rev?: number; paused?: boolean; name?: string } }>()
  let started = false
  let sent: unknown
  const store = new BotStore({ id: 'c1', name: 'Mac', signKey: '', urls: [] }, true, () => ({ call: () => new Promise(() => {}), stream: () => () => {} }), '2.54.0')
  store.connection = { kind: 'online' }
  store.client = { call: (method: string, body?: unknown) => {
    if (method === 'updateBot') { started = true; sent = body; return update.promise }
    return Promise.resolve({})
  } } as HostClient
  return { store, update, started: () => started, sent: () => sent }
}

test('a pause sends only the id and the paused flag, not the menu snapshot', async () => {
  const { store, started, sent } = pausingStore()
  const current = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false, model: 'new-model' })
  store.bots.set(current.id, current)
  const menu = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false, model: 'old-model' })

  store.setPaused(menu, true)
  await ticks()

  assert.equal(started(), true)
  assert.deepEqual(sent(), { id: 'b', paused: true })
})

test('a failed pause does not bring back a bot deleted while the update was in flight', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'gone', name: 'Gone', rev: 3, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  assert.equal(store.bots.get('gone')?.paused, true)

  store.delete(bot)
  assert.equal(store.bots.has('gone'), false)
  update.reject(new Error('refused'))
  await ticks()

  assert.equal(store.bots.has('gone'), false)
})

test('a pause answer with no bot does not bring back a bot deleted while the update was in flight', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'gone', name: 'Gone', rev: 3, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.delete(bot)
  update.resolve({})
  await ticks()

  assert.equal(store.bots.has('gone'), false)
})

test('a pause echo does not bring back a bot deleted while the update was in flight', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'gone', name: 'Gone', rev: 3, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.delete(bot)
  update.resolve({ bot: { id: 'gone', name: 'Gone', rev: 4, paused: true } })
  await ticks()

  assert.equal(store.bots.has('gone'), false)
})

test('a late pause echo with an older rev does not replace a newer paused value', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.bots.set('b', { ...store.bots.get('b')!, paused: false, rev: 8 })

  update.resolve({ bot: { id: 'b', name: 'Bee', rev: 4, paused: true } })
  await ticks()

  assert.equal(store.bots.get('b')?.paused, false)
  assert.equal(store.bots.get('b')?.rev, 8)
})

test('a failed pause puts only the paused flag back and keeps a newer name', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.bots.set('b', { ...store.bots.get('b')!, name: 'Newer' })
  update.reject(new Error('refused'))
  await ticks()

  assert.equal(store.bots.get('b')?.paused, false)
  assert.equal(store.bots.get('b')?.name, 'Newer')
})

test('an echo that leaves paused out is not shown as paused', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  update.resolve({ bot: { id: 'b', name: 'Bee', rev: 5 } })
  await ticks()

  assert.equal(store.bots.get('b')?.paused, false)
  assert.equal(store.lastError, 'Not applied on this computer')
})

test('an echo with no rev updates paused and leaves the rest of the row', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  update.resolve({ bot: { id: 'b', name: 'Renamed', paused: true } })
  await ticks()

  assert.equal(store.bots.get('b')?.name, 'Bee')
  assert.equal(store.bots.get('b')?.paused, true)
  assert.equal(store.bots.get('b')?.rev, 5)
})

test('an echo with no rev after a newer rename keeps that name and takes the echo paused', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.bots.set('b', { ...store.bots.get('b')!, name: 'Newer', rev: 8 })

  update.resolve({ bot: { id: 'b', name: 'Renamed', paused: true } })
  await ticks()

  assert.equal(store.bots.get('b')?.name, 'Newer')
  assert.equal(store.bots.get('b')?.paused, true)
  assert.equal(store.bots.get('b')?.rev, 8)
})

test('a pause echo whose rev is not older than the row replaces that row', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)

  update.resolve({ bot: { id: 'b', name: 'Bee', rev: 5, paused: true } })
  await ticks()

  assert.equal(store.bots.get('b')?.paused, true)
  assert.equal(store.bots.get('b')?.rev, 5)
})

test('a stale numeric echo does not raise Not applied over a newer row', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.bots.set('b', { ...store.bots.get('b')!, name: 'Newer', paused: false, rev: 8 })
  update.resolve({ bot: { id: 'b', name: 'Bee', rev: 4 } })
  await ticks()

  assert.equal(store.bots.get('b')?.name, 'Newer')
  assert.equal(store.bots.get('b')?.paused, false)
  assert.equal(store.bots.get('b')?.rev, 8)
  assert.equal(store.lastError, null)
})

test('a pause that throws after a newer event leaves that row', async () => {
  const { store, update, started } = pausingStore()
  const bot = normalizeBot({ id: 'b', name: 'Bee', rev: 5, paused: false })
  store.bots.set(bot.id, bot)

  store.setPaused(bot, true)
  await ticks()
  assert.equal(started(), true)
  store.bots.set('b', { ...store.bots.get('b')!, name: 'Newer', paused: true, rev: 8 })
  update.reject(new Error('refused'))
  await ticks()

  assert.equal(store.bots.get('b')?.name, 'Newer')
  assert.equal(store.bots.get('b')?.paused, true)
  assert.equal(store.bots.get('b')?.rev, 8)
})
