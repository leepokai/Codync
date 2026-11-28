import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { HostAlert } from '@shared/alerts'
import type { Entry } from '@shared/models'
import type { HostEvent, HostTransport } from '../client/host-client.ts'
import { heldOutside, waitingMessages } from '../views/chat/queued-messages.ts'

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
const { HostClient, parseEvent } = await import('../client/host-client.ts')

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

const heldEntry = (id = 'held-1'): Entry => ({
  id, seq: 1, botId: 'b', rev: 1, kind: 'user', turn: 1,
  data: { text: 'later', status: 'scheduled', notBefore: 9_999_999_999_999 }, createdAt: 0, updatedAt: 0,
})

test('the events decoder carries the held-message snapshot and clear-time sends its sheet revision', async () => {
  const held = heldEntry()
  const decoded = parseEvent(JSON.stringify({
    type: 'hello', hostId: 'h', rev: 8, scheduleSend: true, held: [held], catchUp: 0,
  }))
  assert.equal(decoded?.type, 'hello')
  if (decoded?.type !== 'hello') return
  assert.equal(decoded.scheduleSend, true)
  assert.deepEqual(decoded.held, [held])

  const calls: { method: string; body: unknown }[] = []
  const transport: HostTransport = {
    call: (method, body) => {
      calls.push({ method, body })
      return Promise.resolve({ entry: { ...held, rev: 9, data: { ...held.data, status: 'queued' } } })
    },
    stream: () => () => {},
  }
  const client = new HostClient(transport)
  await client.sendScheduledNow('b', held.id, held.rev, 'edited in this sheet')
  assert.deepEqual(calls, [{
    method: 'sendScheduledNow',
    body: { botId: 'b', entryId: held.id, expectedRev: held.rev, text: 'edited in this sheet' },
  }])
})

test('updateScheduled sends the revision the sheet opened at', async () => {
  const held = heldEntry()
  const calls: { method: string; body: unknown }[] = []
  const transport: HostTransport = {
    call: (method, body) => {
      calls.push({ method, body })
      return Promise.resolve({ entry: held })
    },
    stream: () => () => {},
  }
  const client = new HostClient(transport)
  await client.updateScheduled('b', held.id, { text: 'edited', notBefore: 9, expectedRev: held.rev })
  assert.deepEqual(calls, [{
    method: 'updateScheduled',
    body: { botId: 'b', entryId: held.id, text: 'edited', notBefore: 9, expectedRev: held.rev },
  }])
})

test('a held entry event updates its below-window hello copy, then removes it on cancel and fire', () => {
  const { store, receive } = storeWithAlerts()
  const held = heldEntry()
  const newer: Entry = { ...held, id: 'newer', seq: 100, rev: 5, data: { text: 'newest in the loaded window', status: 'sent' } }
  store.entries.set('b', [newer])
  ;(store as unknown as { floors: Map<string, number> }).floors = new Map([['b', 100]])
  ;(store as unknown as { hello: unknown }).hello = { version: '2.54.0', scheduleSend: true, held: [held] }

  const edited = { ...held, rev: 6, data: { ...held.data, text: 'edited outside the window', notBefore: 8_888_888_888_888 } }
  receive({ type: 'entry', entry: edited })
  assert.deepEqual(store.hello?.held, [edited], 'the event updates the only visible copy')
  assert.deepEqual(store.entries.get('b'), [newer], 'the old row does not open a gap in history paging')

  const cancelled = { ...edited, rev: 7, data: { ...edited.data, status: 'cancelled', removed: true } }
  receive({ type: 'entry', entry: cancelled })
  assert.deepEqual(store.hello?.held, [], 'a cancellation below the window cannot reappear')

  receive({ type: 'entry', entry: edited })
  const fired = { ...edited, rev: 8, data: { ...edited.data, status: 'queued', scheduledFired: true } }
  receive({ type: 'entry', entry: fired })
  assert.deepEqual(store.hello?.held, [], 'scheduledFired delivery also leaves no stale scheduled row')
})

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

test('a taken-back row the chat has dropped does not come back from a stale held snapshot', () => {
  const { store } = storeWithAlerts()
  const held = heldEntry()
  ;(store as unknown as { hello: unknown }).hello = { version: '2.54.0', scheduleSend: true, held: [held] }
  // The chat kept only the hidden tombstone: what a cancel from another surface leaves, its snapshot unchanged.
  store.entries.set('b', [{ ...held, rev: 2, data: { ...held.data, status: 'cancelled', removed: true } }])
  assert.deepEqual(store.allEntries('b'), [], 'the chat no longer shows the row')
  const shown = store.allEntries('b')
  assert.deepEqual(heldOutside(shown, 'b', store.hello?.held, store.takenBack('b')), [], 'so the tray does not either')
  // With no removal behind it the same snapshot still reaches the tray from below the loaded window.
  store.entries.set('b', [])
  const below = heldOutside(store.allEntries('b'), 'b', store.hello?.held, store.takenBack('b'))
  assert.deepEqual(below.map((e) => e.id), [held.id], 'a held message nothing removed still shows')
})

test('a schedule made below this window joins the tray and stays out of the transcript', () => {
  const { store, receive } = storeWithAlerts()
  const newer: Entry = { ...heldEntry('newer'), seq: 100, rev: 5, data: { text: 'newest in the loaded window', status: 'sent' } }
  store.entries.set('b', [newer])
  ;(store as unknown as { floors: Map<string, number> }).floors = new Map([['b', 100]])
  ;(store as unknown as { hello: unknown }).hello = { version: '2.54.0', scheduleSend: true, held: [] }

  const scheduled = heldEntry('from-elsewhere')
  receive({ type: 'entry', entry: scheduled })
  assert.deepEqual(store.hello?.held, [scheduled], 'another window\'s schedule is visible without reconnecting')
  assert.deepEqual(store.entries.get('b'), [newer], 'it does not open a gap in history paging')
  const shown = store.allEntries('b')
  assert.deepEqual(
    waitingMessages([...shown, ...heldOutside(shown, 'b', store.hello?.held)], false).map((e) => e.id),
    ['from-elsewhere'],
  )

  receive({
    type: 'entry',
    entry: { ...newer, id: 'old-rewrite', seq: 4, rev: 6, data: { text: 'a reaction, not a schedule', status: 'sent' } },
  })
  assert.deepEqual(store.hello?.held?.map((e) => e.id), ['from-elsewhere'], 'an old rewrite is not a held message')
  assert.deepEqual(store.entries.get('b'), [newer])
})

test('a fired schedule moves its existing row to the delivery end of the chat', () => {
  const { store, receive } = storeWithAlerts()
  const held = heldEntry()
  const reply: Entry = {
    ...held, id: 'reply', seq: 2, rev: 2, kind: 'agent', turn: 2,
    data: { text: 'work that happened meanwhile', final: true }, createdAt: 2,
  }
  store.entries.set('b', [held, reply])
  const fired: Entry = {
    ...held, seq: 3, rev: 3, turn: 3, createdAt: 3,
    data: { text: 'later', status: 'queued' },
  }
  receive({ type: 'entry', entry: fired })
  assert.deepEqual(store.allEntries('b').map((e) => e.id), ['reply', 'held-1'])
  assert.equal(store.allEntries('b')[1]?.createdAt, 3)
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

test("a step run on its own is read again on its event, only while its workflow is open", async () => {
  const step = { id: 's1', nodeId: 'a', status: 'done' }
  const asked: unknown[] = []
  const transport: HostTransport = {
    call: (method, body) => (method === 'workflowStep' ? (asked.push(body), Promise.resolve({ step })) : new Promise(() => {})),
    stream: () => () => {},
  }
  const { store, receive } = storeWithAlerts()
  const client = new HostClient(transport)
  Object.defineProperty(store, 'live', { get: () => client })
  const event = parseEvent(JSON.stringify({ type: 'workflow', change: 'step', workflowId: 'w1', runId: null, stepId: 's1' }))
  assert.deepEqual(event, { type: 'workflow', change: 'step', workflowId: 'w1', runId: null, stepId: 's1', enabled: undefined })
  store.setWorkflowSession({ id: 'w2', dirty: false, runId: null, followLive: true })
  receive(event)
  await new Promise((resolve) => setTimeout(resolve))
  assert.deepEqual(asked, [])
  store.setWorkflowSession({ id: 'w1', dirty: false, runId: null, followLive: true })
  receive(event)
  await new Promise((resolve) => setTimeout(resolve))
  assert.deepEqual(asked, [{ stepId: 's1' }])
  assert.deepEqual(store.workflowSoloStep, step)
})

test('a models event without its agent or list is passed over', () => {
  assert.equal(parseEvent(JSON.stringify({ type: 'models', loginProfile: null, models: { models: [] } })), null)
  assert.equal(parseEvent(JSON.stringify({ type: 'models', backend: 'codex', models: {} })), null)
})
