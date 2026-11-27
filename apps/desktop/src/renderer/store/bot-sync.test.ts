import assert from 'node:assert/strict'
import ts from 'typescript'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test, type TestContext } from 'node:test'
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
  window: { addEventListener: () => {} },
  localStorage: { getItem: (k: string) => storage.get(k) ?? null, setItem: (k: string, v: string) => void storage.set(k, v), removeItem: (k: string) => void storage.delete(k) },
})

const { BotSync } = await import('./bot-sync.ts')
const { BotStore } = await import('./bot-store.ts')
const { HostClient, parseEvent } = await import('../client/host-client.ts')
const { draftOf } = await import('@shared/models')

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

const nextTurn = () => new Promise<void>((resolve) => setImmediate(resolve))
const hostHello = { version: '2.54.0', name: 'Mac', backends: [] }

function storeWithStream(t: TestContext, call: HostTransport['call'] = async () => hostHello) {
  let onData!: (line: string) => void
  let onEnd!: (error: null) => void
  let opened: (() => void) | undefined
  const requests: number[] = []
  const transport: HostTransport = {
    call,
    stream: (path, receive, end) => {
      requests.push(Number(new URL(path, 'http://localhost').searchParams.get('since')))
      onData = receive
      onEnd = end
      opened?.()
      opened = undefined
      return () => {}
    },
  }
  const store = new BotStore({ id: 'stream', name: 'Mac', signKey: '', urls: [] }, false, () => transport, '2.54.0')
  t.after(() => store.retire())
  const send = (event: object) => onData(JSON.stringify(event))
  const nextStream = () => new Promise<void>((resolve) => { opened = resolve })
  const connect = async (rev: number, catchUp?: number, hostId = 'host') => {
    const ready = nextStream()
    store.restartStream()
    await ready
    const receive = onData
    send({ type: 'hello', hostId, rev, catchUp })
    return (event: object) => receive(JSON.stringify(event))
  }
  const update = (id: string, rev: number, status: Bot['status'], completionReplyId?: string | null) => {
    send({ type: 'bot', rev, bot: { id, rev, status, completionReplyId } })
    return store.completionFor(store.bots.get(id)!)
  }
  return { store, send, connect, update, requests, nextStream, drop: () => onEnd(null) }
}

const restoredEntry = (id: string, rev: number, text: string, clientNonce?: string): Entry => ({
  id, botId: 'a', seq: 1, rev, turn: 1, kind: clientNonce ? 'user' : 'agent',
  data: { text, clientNonce }, createdAt: 1, updatedAt: rev,
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

test('a live completion can precede the hello revision after the counted catch-up', async (t) => {
  const { store, send, connect, update } = storeWithStream(t)
  await connect(120, 3)
  send({ type: 'icons' })
  assert.equal(update('a', 117, 'working', null), null)
  send({ type: 'futureEvent' })
  assert.equal(update('a', 119, 'idle', 'during-connect'), 'during-connect')
  store.consumeCompletion('a', 'during-connect')
  assert.equal(update('a', 119, 'idle', 'during-connect'), null)
  assert.equal(update('a', 122, 'working', null), null)
  assert.equal(update('a', 124, 'idle', 'next-reply'), 'next-reply')
})

test('a restored host discards the abandoned stream and rebuilds the bot and chat mirror', async (t) => {
  const { store, send, connect, update, requests } = storeWithStream(t)
  const entry = (id: string, rev: number, text: string): Entry => ({
    id, botId: 'a', seq: id === 'saved' ? 1 : rev, rev, turn: 1, kind: 'agent', data: { final: true, text }, createdAt: rev, updatedAt: rev,
  })
  const bot = (id: string, name: string, rev: number, status: Bot['status'], completionReplyId: string | null = null) =>
    ({ type: 'bot', rev, bot: { id, name, rev, status, completionReplyId } })

  await connect(130, 4)
  send(bot('a', 'Renamed Alpha', 121, 'idle', 'abandoned-reply'))
  send({ type: 'entry', entry: entry('saved', 122, 'Edited later') })
  send({ type: 'entry', entry: entry('abandoned-reply', 126, 'Written later') })
  send(bot('later', 'Created later', 128, 'idle'))
  const local: Entry = {
    ...entry('local-unsent', 0, 'Pending send'), kind: 'user', seq: Number.MAX_SAFE_INTEGER, data: { text: 'Pending send', status: 'failed' },
  }
  store.entries.get('a')!.push(local)
  store.composerDrafts.set('a', 'Draft to keep')
  t.after(() => storage.clear())

  const abandoned = await connect(24, 1)
  abandoned(bot('extra', 'From the abandoned stream', 129, 'idle'))
  await new Promise<void>((resolve) => setImmediate(resolve))
  assert.deepEqual(requests, [0, 128, 0])

  send({ type: 'hello', hostId: 'host', rev: 24, catchUp: 4 })
  send({ type: 'icons' })
  send(bot('b', 'Beta', 19, 'idle', 'historical-reply'))
  send(bot('a', 'Alpha', 21, 'working'))
  send({ type: 'entry', entry: entry('saved', 22, 'Before the edit') })
  assert.deepEqual([...store.bots.keys()], ['b', 'a'])
  assert.equal(store.bots.get('a')?.name, 'Alpha')
  assert.equal(store.bots.get('a')?.status, 'working')
  assert.deepEqual(store.allEntries('a'), [entry('saved', 22, 'Before the edit'), local])
  assert.equal(store.composerDrafts.get('a'), 'Draft to keep')
  assert.equal(store.completionFor(store.bots.get('b')!), null)
  assert.equal(update('b', 22, 'idle', 'historical-reply'), null)
  assert.equal(update('a', 23, 'idle', 'fresh-reply'), 'fresh-reply')
  store.consumeCompletion('a', 'fresh-reply')
  assert.equal(update('a', 23, 'idle', 'fresh-reply'), null)
  assert.equal(update('a', 25, 'idle', 'fresh-reply'), null)

  await connect(25, 0)
  await new Promise<void>((resolve) => setImmediate(resolve))
  assert.deepEqual(requests, [0, 128, 0, 25], 'a normal reconnect keeps its cursor')
})

test('a second zero-cursor fetch discards the first partial replay', { timeout: 5000 }, async (t) => {
  const { store, send, connect, update, requests, nextStream, drop } = storeWithStream(t)
  await connect(130, 1)
  update('a', 128, 'idle')
  await connect(26, 0)
  await nextTurn()
  send({ type: 'hello', hostId: 'host', rev: 26, catchUp: 2 })
  send({ type: 'entry', entry: restoredEntry('shared', 25, 'Partial copy') })
  const retried = nextStream()
  drop()
  await retried
  send({ type: 'hello', hostId: 'host', rev: 24, catchUp: 2 })
  send({ type: 'entry', entry: restoredEntry('shared', 21, 'Restored copy') })
  update('a', 22, 'idle')
  assert.deepEqual(requests, [0, 128, 0, 0])
  assert.equal(store.allEntries('a')[0]?.data.text, 'Restored copy')
})

for (const counted of [true, false]) {
  test(`an interrupted ${counted ? 'counted' : 'legacy'} recovery deletion keeps the draft and retry`, { timeout: 5000 }, async (t) => {
    const uploads: string[] = []
    const sends: string[] = []
    const { store, send, connect, update, requests, nextStream, drop } = storeWithStream(t, async (method, body) => {
      if (method === 'upload') uploads.push((body as { data: string }).data)
      if (method === 'send') {
        sends.push((body as { clientNonce: string }).clientNonce)
        return new Promise(() => {})
      }
      return hostHello
    })
    await connect(130, 1)
    update('a', 128, 'idle')
    store.selection = 'a'
    const file = { id: 'file', name: 'report.txt', data: Uint8Array.of(65) }
    store.composerDrafts.set('a', 'Unsent draft')
    store.draftFiles.update('a', () => [file])
    store.send('Retry me', 'a', { nonce: 'pending', files: [file] })
    await nextTurn()

    await connect(26, 0)
    await nextTurn()
    send({ type: 'hello', hostId: 'host', rev: 26, catchUp: counted ? 2 : undefined })
    send({ type: 'bot', bot: { id: 'a', rev: 25, deleted: true } })
    assert.equal(store.bots.has('a'), false)
    const retried = nextStream()
    drop()
    await retried
    send({ type: 'hello', hostId: 'host', rev: 24, catchUp: counted ? 1 : undefined })
    update('a', 22, 'idle')
    if (!counted) update('a', 27, 'idle')
    assert.deepEqual(requests, [0, 128, 0, 0])
    assert.deepEqual([store.composerDrafts.get('a'), store.draftFiles.get('a'), store.selection], ['Unsent draft', [file], 'a'])
    const pending = store.allEntries('a')[0]!
    assert.equal(pending.data.status, 'failed')
    store.retry(pending)
    await nextTurn()
    assert.deepEqual(uploads, ['QQ==', 'QQ=='])
    assert.deepEqual(sends, ['pending', 'pending'])

    await connect(30, 0)
    send({ type: 'bot', bot: { id: 'a', rev: 31, deleted: true } })
    assert.equal(store.composerDrafts.get('a'), '')
    assert.deepEqual(store.draftFiles.get('a'), [])
    assert.equal(store.selection, null)
    store.retry(pending)
    await nextTurn()
    assert.deepEqual(uploads, ['QQ==', 'QQ=='], 'a removed retry no longer has its upload bytes')
    assert.deepEqual(store.allEntries('a'), [])
  })

  test(`a complete ${counted ? 'counted' : 'legacy'} recovery clears only bots still deleted`, async (t) => {
    const { store, send, connect, update } = storeWithStream(t, async (method) => method === 'send' ? new Promise(() => {}) : hostHello)
    await connect(130, 2)
    update('a', 127, 'idle')
    update('b', 128, 'idle')
    const file = { id: 'file', name: 'report.txt', data: Uint8Array.of(65) }
    for (const id of ['a', 'b']) {
      store.composerDrafts.set(id, `Draft ${id}`)
      store.draftFiles.update(id, () => [file])
      store.send(`Pending ${id}`, id, { nonce: id })
    }
    await connect(26, 0)
    await nextTurn()
    send({ type: 'hello', hostId: 'host', rev: 26, catchUp: counted ? 5 : undefined })
    send({ type: 'bot', bot: { id: 'a', rev: 21, deleted: true } })
    send({ type: 'bot', bot: { id: 'b', rev: 22, deleted: true } })
    send({ type: 'bot', bot: { id: 'a', rev: 20, status: 'idle' } })
    update('b', 23, 'idle')
    assert.equal(store.composerDrafts.get('a'), 'Draft a', 'the snapshot is still incomplete')
    assert.equal(store.bots.has('a'), false, 'a stale bot row cannot supersede the deletion')
    update('b', counted ? 24 : 27, 'idle')
    assert.equal(store.composerDrafts.get('a'), '')
    assert.deepEqual(store.draftFiles.get('a'), [])
    assert.deepEqual(store.allEntries('a'), [])
    assert.equal(store.composerDrafts.get('b'), 'Draft b')
    assert.deepEqual(store.draftFiles.get('b'), [file])
    assert.equal(store.allEntries('b')[0]?.data.clientNonce, 'b')
  })
}

for (const counted of [true, false]) {
  test(`a receipt in an interrupted ${counted ? 'counted' : 'legacy'} recovery leaves the text and files retryable`, { timeout: 5000 }, async (t) => {
    const uploads: Record<string, unknown>[] = []
    const sends: { body: Record<string, unknown>; reply: (answer: unknown) => void }[] = []
    const { store, send, connect, update, requests, nextStream, drop } = storeWithStream(t, async (method, body) => {
      const params = body as Record<string, unknown>
      if (method === 'upload') uploads.push(params)
      if (method === 'send') return new Promise((reply) => sends.push({ body: params, reply }))
      return hostHello
    })
    await connect(130, 1)
    update('a', 128, 'idle')
    store.send('Retry me', 'a', { nonce: 'pending', files: [{ id: 'file', name: 'report.txt', data: Uint8Array.of(65) }] })
    await nextTurn()

    await connect(26, 0)
    await nextTurn()
    send({ type: 'hello', hostId: 'host', rev: 26, catchUp: counted ? 3 : undefined })
    update('a', 21, 'idle')
    send({ type: 'entry', entry: restoredEntry('partial', 23, 'Retry me', 'pending') })
    const retried = nextStream()
    drop()
    await retried
    send({ type: 'hello', hostId: 'host', rev: 24, catchUp: counted ? 1 : undefined })
    update('a', 22, 'idle')
    assert.deepEqual(requests, [0, 128, 0, 0])
    assert.deepEqual(store.allEntries('a').map((e) => [e.id, e.data.text, e.data.status]), [['local-pending', 'Retry me', 'failed']])

    store.retry(store.allEntries('a')[0]!)
    await nextTurn()
    assert.deepEqual(uploads.map((u) => [u.botId, u.name, u.data]), Array(2).fill(['a', 'report.txt', 'QQ==']))
    assert.deepEqual(sends.map((s) => [s.body.botId, s.body.text, s.body.clientNonce]), Array(2).fill(['a', 'Retry me', 'pending']))
    assert.deepEqual(sends[1]?.body.attachments, [uploads[1]?.uploadId])
    sends[1]!.reply({ entry: restoredEntry('saved', 25, 'Retry me', 'pending') })
    await nextTurn()
    assert.deepEqual(store.allEntries('a').map((e) => e.id), ['saved'], 'an RPC confirms even during recovery')
  })

  test(`a complete ${counted ? 'counted' : 'legacy'} recovery confirms a restored send once`, async (t) => {
    const { store, send, connect, update } = storeWithStream(t, async (method) => method === 'send' ? new Promise(() => {}) : hostHello)
    await connect(130, 1)
    update('a', 128, 'idle')
    store.send('Already saved', 'a', { nonce: 'pending' })
    await connect(26, 0)
    await nextTurn()

    send({ type: 'hello', hostId: 'host', rev: 26, catchUp: counted ? 3 : undefined })
    update('a', 21, 'idle')
    const entry = restoredEntry('saved', 23, 'Already saved', 'pending')
    send({ type: 'entry', entry })
    assert.ok(store.allEntries('a').some((e) => e.id === 'local-pending'))
    update('a', counted ? 24 : 27, 'idle')
    assert.deepEqual(store.allEntries('a').map((e) => e.id), ['saved'])
    send({ type: 'entry', entry })
    assert.deepEqual(store.allEntries('a').map((e) => e.id), ['saved'])
  })
}

test('a restored chat ignores abandoned send and bot replies and can resend with the same files and nonce', async (t) => {
  const calls: { method: string; body: Record<string, unknown>; reply: (answer: unknown) => void }[] = []
  const { store, send, connect, update } = storeWithStream(t, async (method, body) => {
    if (method === 'hello') return hostHello
    if (method === 'upload') {
      calls.push({ method, body: body as Record<string, unknown>, reply: () => {} })
      return {}
    }
    return new Promise((reply) => calls.push({ method, body: body as Record<string, unknown>, reply }))
  })
  await connect(130, 1)
  update('a', 128, 'idle')
  store.send('Pending text', 'a', { nonce: 'same-nonce', files: [{ id: 'file', name: 'report.txt', data: Uint8Array.of(65) }] })
  const save = store.save(draftOf(store.bots.get('a')!)).then(() => true, () => false)
  await nextTurn()
  const oldSend = calls.find((c) => c.method === 'send')!
  const oldSave = calls.find((c) => c.method === 'updateBot')!
  assert.ok(oldSend && oldSave)

  await connect(26, 0)
  await nextTurn()
  send({ type: 'hello', hostId: 'host', rev: 26, catchUp: 1 })
  update('a', 21, 'idle')
  oldSave.reply({ bot: { id: 'a', name: 'Obsolete name', rev: 130, status: 'idle' } })
  await nextTurn()
  assert.deepEqual(store.allEntries('a').map((e) => [e.id, e.data.status]), [['local-same-nonce', 'failed']])
  assert.equal(store.bots.get('a')?.rev, 21)
  assert.equal(await save, false)

  const local = store.allEntries('a')[0]!
  store.retry(local)
  await nextTurn()
  const retried = calls.filter((c) => c.method === 'send').at(-1)!
  assert.notEqual(retried, oldSend)
  assert.equal(retried.body.clientNonce, 'same-nonce')
  assert.equal(retried.body.text, 'Pending text')
  assert.deepEqual(calls.filter((c) => c.method === 'upload').map((c) => c.body.data), ['QQ==', 'QQ=='])
  oldSend.reply({ entry: restoredEntry('lost', 129, 'Old reply', 'same-nonce') })
  await nextTurn()
  assert.deepEqual(store.allEntries('a').map((e) => [e.id, e.data.status]), [['local-same-nonce', 'sending']])
  const confirmed = restoredEntry('confirmed', 23, 'Pending text', 'same-nonce')
  send({ type: 'entry', entry: confirmed })
  retried.reply({ entry: confirmed })
  await nextTurn()
  assert.deepEqual(store.allEntries('a').map((e) => e.id), ['confirmed'])
})

test('restored send receipts leave a different draft and its files alone', async (t) => {
  let oldReply!: (answer: unknown) => void
  const { store, send, connect, update } = storeWithStream(t, async (method) => {
    if (method === 'send') return new Promise((resolve) => { oldReply = resolve })
    return hostHello
  })
  await connect(130, 1)
  update('a', 128, 'idle')
  store.send('Already stored', 'a', { nonce: 'present' })
  await nextTurn()
  store.composerDrafts.set('a', 'New draft')
  t.after(() => storage.clear())
  const file = { id: 'file', name: 'draft.txt', data: Uint8Array.of(65) }
  const drafts = store.draftFiles
  drafts.update('a', () => [file])
  assert.deepEqual(drafts.get('another-chat'), [])

  await connect(24, 0)
  await nextTurn()
  send({ type: 'hello', hostId: 'host', rev: 24, catchUp: 2 })
  update('a', 20, 'idle')
  send({ type: 'entry', entry: restoredEntry('confirmed', 21, 'Already stored', 'present') })
  oldReply({ entry: restoredEntry('obsolete', 129, 'Obsolete reply', 'present') })
  await nextTurn()
  assert.deepEqual(store.allEntries('a').map((e) => e.id), ['confirmed'])
  assert.equal(store.composerDrafts.get('a'), 'New draft')
  assert.deepEqual(store.draftFiles.get('a'), [file])

  await connect(24, 0, 'another-host')
  drafts.update('a', () => [file])
  assert.deepEqual(store.draftFiles.get('a'), [])
})

test('a refused pre-restore roster update cannot undo the restored lead', async (t) => {
  let rejectLead!: (reason: Error) => void
  const { store, send, connect, update } = storeWithStream(t, async (method) => {
    if (method === 'setLead') return new Promise((_resolve, reject) => { rejectLead = reject })
    return hostHello
  })
  await connect(130, 1)
  update('a', 128, 'idle')
  store.setLead(store.bots.get('a')!)
  await nextTurn()

  await connect(24, 0)
  await nextTurn()
  send({ type: 'hello', hostId: 'host', rev: 24, catchUp: 1 })
  send({ type: 'bot', bot: { id: 'a', rev: 21, lead: true } })
  rejectLead(new Error('Abandoned request'))
  await nextTurn()
  assert.equal(store.bots.get('a')?.lead, true)
  assert.equal(store.lastError, null)
})

test('older buffered bot states and duplicate replies stay quiet after catch-up', async (t) => {
  const { store, send, connect, update } = storeWithStream(t)
  await connect(110, 0)
  assert.equal(update('kept', 111, 'idle', 'already-held'), 'already-held')
  await connect(120, 4)
  assert.equal(update('replayed', 116, 'idle', 'in-snapshot'), null)
  send({ type: 'bot', rev: 117, bot: { id: 'deleted', rev: 117, deleted: true } })
  send({ type: 'futureEvent' })
  send({ type: 'usage', usage: { providers: [] } })

  assert.equal(update('kept', 109, 'idle', 'older-reply'), null)
  assert.equal(update('kept', 110, 'working', null), null)
  assert.equal(update('kept', 111, 'idle', 'already-held'), null)
  assert.equal(update('kept', 112, 'working', null), null)
  assert.equal(update('replayed', 113, 'working', null), null)
  send({ type: 'bot', rev: 114, bot: { id: 'deleted', rev: 114, status: 'working' } })
  assert.equal(store.bots.has('deleted'), false)
  assert.equal(update('kept', 115, 'idle', 'already-held'), null)
  assert.equal(update('replayed', 118, 'working', null), null)
  assert.equal(update('replayed', 119, 'idle', 'in-snapshot'), null)
  assert.equal(update('replayed', 121, 'idle', 'in-snapshot'), null, 'later metadata stays quiet')
})

test('the transcript fallback uses the count when present and the hello revision otherwise', async (t) => {
  const { connect, update } = storeWithStream(t)
  await connect(120, 1)
  assert.equal(update('a', 117, 'working'), null)
  assert.equal(update('a', 119, 'idle'), undefined)

  await connect(130)
  assert.equal(update('a', 129, 'working'), null)
  assert.equal(update('a', 130, 'idle'), null)
  assert.equal(update('a', 131, 'working'), undefined)
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
  const connect = (rev: number, catchUp: number) => receive({ type: 'hello', hostId: 'host', rev, usage: { providers: [] }, screen: null, network: null, catchUp })
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

  connect(10, 1)
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

  connect(25, 2)
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
