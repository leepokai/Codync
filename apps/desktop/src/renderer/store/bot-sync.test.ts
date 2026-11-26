import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { HostAlert } from '@shared/alerts'
import type { IslandSummary } from '@shared/island'
import type { HostEvent, HostTransport } from '../client/host-client'
import type { Bot, Entry } from '@shared/models'

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
const { parseEvent } = await import('../client/host-client.ts')

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

test('only a changed completion received live can reach the island', () => {
  const { store, receive } = storeWithAlerts()
  const connect = (rev: number) => receive({ type: 'hello', hostId: 'host', rev, usage: { providers: [] }, screen: null, network: null, catchUp: 1 })
  const update = (rev: number, completionReplyId?: string | null) => {
    const bot = { id: 'a', rev, completionReplyId, unread: 0, waiting: 0 } as Bot
    receive({ type: 'bot', bot })
    return store.completionFor(bot)
  }

  connect(10)
  assert.equal(update(9, 'before-connect'), null)
  assert.equal(update(11, 'before-connect'), null, 'new metadata for a replayed completion stays quiet')
  assert.equal(update(12, null), null)
  assert.equal(update(13, 'live'), 'live')
  assert.equal(update(14, 'live'), 'live', 'metadata before the next summary keeps the live completion')
  assert.equal(update(15, null), 'live', 'a summary during the next turn can still deliver the live completion')
  store.consumeCompletion('a', 'older')
  assert.equal(update(16, null), 'live', 'consuming an older id keeps the pending reply')
  store.consumeCompletion('a', 'live')
  assert.equal(update(17, null), null)
  assert.equal(update(18, 'before-delete'), 'before-delete')
  receive({ type: 'botDeleted', id: 'a', rev: 19 })
  assert.equal(update(20, null), null, 'deletion clears the pending reply')
  assert.equal(update(21, 'before-reconnect'), 'before-reconnect')

  connect(25)
  assert.equal(update(25, 'while-offline'), null)
  assert.equal(update(26, 'while-offline'), null)
  assert.equal(update(27), undefined, 'an older host still uses the transcript fallback for live updates')
  store.retire()
})

test('a live reply appears once when the next turn starts between island summaries', async (t) => {
  Object.assign(globalThis, {
    window: { addEventListener: () => {}, codync: { speech: { available: false }, island: { setShape: () => {} } } },
  })
  t.after(() => { Reflect.deleteProperty(globalThis, 'window') })
  const { IslandModel } = await import('../views/island/island-model.ts')
  const { FINISHED_MS, islandReply } = await import('@shared/island')
  const { store, receive } = storeWithAlerts()
  t.after(() => store.retire())
  t.mock.timers.enable({ apis: ['setTimeout'] })

  const island = new IslandModel()
  const connect = (rev: number) => receive({ type: 'hello', hostId: 'host', rev, usage: { providers: [] }, screen: null, network: null, catchUp: 1 })
  const update = (rev: number, status: Bot['status'], completionReplyId: string | null, workingChat: string | null = null) => {
    const bot = { id: 'a', rev, status, completionReplyId, workingChat, unread: 0, waiting: 0 } as Bot
    receive({ type: 'bot', bot })
  }
  const save = (id: string, rev: number, text: string) => {
    const entry: Entry = { id, botId: 'a', seq: rev, rev, turn: 1, kind: 'agent', data: { final: true, text }, createdAt: rev, updatedAt: rev }
    receive({ type: 'entry', entry })
    return entry
  }
  const sample = () => {
    const bot = store.bots.get('a')!
    const id = store.completionFor(bot)
    const completion = typeof id === 'string' ? islandReply(store.allEntries(bot.id), undefined, id) : id
    const summary: IslandSummary = {
      bots: [{
        ref: 'c1/a', name: 'Alpha', shape: 'blob', color: 'blue', chat: bot.status, status: bot.status,
        away: Boolean(bot.workingChat && bot.workingChat !== bot.id),
        completion,
        backend: 'Custom', folder: '', detail: '', steps: [], error: null,
      }],
      permissions: [], usage: [],
    }
    ;(island as unknown as { apply(summary: IslandSummary): void }).apply(summary)
    if (completion) store.consumeCompletion(bot.id, completion.id)
  }

  connect(10)
  update(10, 'idle', null)
  sample()
  update(11, 'working', null, 'a')
  sample()
  assert.equal(island.mode, 'compact')

  update(13, 'idle', 'first-reply')
  update(14, 'working', null, 'a')
  save('unrelated', 15, 'Other visible text')
  sample()
  assert.equal(island.mode, 'compact', 'the saved reply has not arrived yet')
  const reply = save('first-reply', 12, 'The report')
  sample()
  assert.equal(island.mode, 'finished', 'the idle summary was coalesced into the next working summary')
  assert.equal(island.notice?.text, 'The report')
  t.mock.timers.tick(FINISHED_MS - 1)
  update(16, 'working', null, 'a')
  sample()
  assert.equal(island.mode, 'finished')
  t.mock.timers.tick(1)
  assert.equal(island.mode, 'compact')
  sample()
  assert.equal(island.mode, 'compact', 'another summary of the same reply stays quiet')

  update(17, 'idle', null)
  update(18, 'working', null, 'room')
  sample()
  receive({ type: 'entry', entry: { ...reply, rev: 19, updatedAt: 19, data: { ...reply.data, reactions: ['👍'] } } })
  update(20, 'idle', null)
  sample()
  assert.equal(island.mode, 'hidden', 'delegated work and reactions leave no completion')

  connect(25)
  save('offline-reply', 24, 'Reply while disconnected')
  update(25, 'idle', 'offline-reply')
  sample()
  update(26, 'idle', 'offline-reply')
  sample()
  assert.equal(island.mode, 'hidden', 'catch-up and later metadata stay quiet')

  update(27, 'working', null, 'a')
  const next = save('next-reply', 28, 'The report')
  update(29, 'idle', next.id)
  sample()
  assert.equal(island.mode, 'finished', 'a distinct saved reply with identical text still appears')
  assert.equal(island.notice?.text, 'The report')
})

test("a model list the computer read again replaces that login's, and only that login's", () => {
  const { store, receive } = storeWithAlerts()
  const list = (id: string) => ({ models: [{ id, name: id }] })
  store.keepModels('gemini', null, list('flash'))
  store.keepModels('gemini', 'pro-7', list('neon'))
  const event = parseEvent(JSON.stringify({ type: 'models', backend: 'gemini', loginProfile: null, models: list('argon') }))
  assert.ok(event)
  receive(event)
  assert.deepEqual(store.modelsFor('gemini', null), list('argon'))
  assert.deepEqual(store.modelsFor('gemini', 'pro-7'), list('neon'))
  assert.equal(store.modelsFor('codex', null), null)
})

test('a models event without its agent or list is passed over', () => {
  assert.equal(parseEvent(JSON.stringify({ type: 'models', loginProfile: null, models: { models: [] } })), null)
  assert.equal(parseEvent(JSON.stringify({ type: 'models', backend: 'codex', models: {} })), null)
})
