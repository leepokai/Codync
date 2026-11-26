import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { HostAlert } from '@shared/alerts'
import type { Entry } from '@shared/models'
import type { HostEvent, HostTransport } from '../client/host-client'

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

// The window's storage, empty: no cached mirror to load.
const storage = new Map<string, string>()
Object.assign(globalThis, {
  localStorage: { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => void storage.set(k, v), removeItem: (k: string) => void storage.delete(k) },
})

const { BotSync } = await import('./bot-sync.ts')

/** A store whose host is never reached, and the alerts it passes on. */
function storeWithAlerts() {
  const transport: HostTransport = { call: () => new Promise(() => {}), stream: () => () => {} }
  const store = new BotSync({ id: 'c1', name: 'Mac', signKey: '', urls: [] }, true, () => transport, '2.51.0')
  const passed: HostAlert[] = []
  store.onAlert = (alert) => passed.push(alert)
  // `apply` is the events stream's handler.
  const receive = (event: HostEvent) => (store as unknown as { apply(e: HostEvent): void }).apply(event)
  return { store, passed, receive }
}

const alert = (botId: string): HostAlert => ({ botId, kind: 'done', title: botId, body: 'Finished.' })

test('a held message that stops being scheduled leaves hello.held, and an older hello keeps scheduleSend', () => {
  const { store, receive } = storeWithAlerts()
  const held: Entry = {
    id: 'held-1', seq: 1, botId: 'b', rev: 1, kind: 'user', turn: 1,
    data: { text: 'later', status: 'scheduled', notBefore: 9_999_999_999_999 }, createdAt: 0, updatedAt: 0,
  }
  ;(store as unknown as { hello: unknown }).hello = { version: '2.51.0', scheduleSend: true, held: [held] }
  // An older host's hello omits the capability: the one already known must stand.
  receive({
    type: 'hello', hostId: 'h', rev: 0, usage: {} as never, screen: null, network: null,
    catchUp: null, scheduleSend: undefined, held: [held],
  } as HostEvent)
  assert.equal(store.hello?.scheduleSend, true)
  // The host says it fired or was cancelled: its scheduled copy in hello.held must go with it.
  receive({
    type: 'entry',
    entry: { ...held, rev: 2, data: { ...held.data, status: 'cancelled', removed: true } },
  })
  assert.deepEqual(store.hello?.held, [])
})

test("an alert passes on unless that bot's chat is on screen in a focused window", () => {
  const { store, passed, receive } = storeWithAlerts()
  store.setReading('view', 'open', true)
  receive({ type: 'alert', alert: alert('open') })
  receive({ type: 'alert', alert: alert('other') })
  assert.deepEqual(passed.map((a) => a.botId), ['other'])

  store.setReading('view', 'open', false)
  receive({ type: 'alert', alert: alert('open') })
  assert.deepEqual(passed.map((a) => a.botId), ['other', 'open'])
})
