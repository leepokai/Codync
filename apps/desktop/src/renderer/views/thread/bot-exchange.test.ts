import assert from 'node:assert/strict'
import { test } from 'node:test'
import { botConversation, botExchange, buildBotConversation, groupExchanges, groupSummary, isOutgoing, peerOf, verbOf } from './bot-exchange.ts'
import type { Entry, EntryData } from '../../../shared/models.ts'

interface Extra {
  intent?: string
  sourceBotId?: string
  targetBotId?: string
  request?: string
  outcome?: string
  reply?: string
  heading?: string
}

const notice = (id: string, chat: string, seq: number, status: string, extra: Extra = {}, createdAt = 1000, rev = 1): Entry => {
  const sourceBotId = extra.sourceBotId ?? 'egan'
  const targetBotId = extra.targetBotId ?? 'owen'
  const intent = extra.intent ?? 'message'
  const outgoing = chat === sourceBotId
  const verb = intent === 'ask' ? (outgoing ? 'Asked ' : 'Request from ') : outgoing ? 'Messaged ' : 'Message from '
  const heading = extra.heading ?? `${verb}${outgoing ? 'Owen' : 'Egan'}: ${extra.request ?? 'Inspect this.'}`
  const data: EntryData = {
    text: extra.outcome === undefined ? heading : `${heading}\n${extra.outcome}`,
    heading, status, delegationId: 'd', sourceBotId, targetBotId,
    botMessage: { sourceBotId, targetBotId, text: extra.request ?? 'Inspect this.', reply: extra.reply,
      detail: status === 'failed' || status === 'cancelled' ? extra.outcome : undefined },
  }
  return { id, seq, botId: chat, rev, kind: 'notice', turn: 0, createdAt, updatedAt: createdAt, data }
}
const parsed = (e: Entry) => botExchange(e)!

test('bot replies stay in the popup while independent reports stay in the main chat', () => {
  const ask = notice('ask', 'owen', 1, 'completed', { intent: 'ask', reply: 'Answer for Egan' })
  const trace: Entry = { ...ask, id: 'trace', seq: 2, kind: 'agent', data: { text: 'Answer for Egan' } }
  const independent = notice('message', 'owen', 3, 'completed')
  const report: Entry = { ...trace, id: 'report', seq: 4, data: { text: 'Report for the user', final: true } }
  const entries = [ask, trace, independent, report]
  const main = groupExchanges(entries)
  assert.deepEqual(main.map((slot) => slot.entry.id), ['ask', 'report'])
  const popup = buildBotConversation(botConversation([], entries, 'egan'))
  assert.deepEqual(popup.filter((row) => row.kind === 'message').map((row) => row.text), [
    'Inspect this.', 'Answer for Egan', 'Inspect this.',
  ])
})

test('decodes the wire JSON and reads direction and verb from either chat', () => {
  const wire = JSON.parse('{"id":"n","seq":1,"botId":"egan","rev":3,"kind":"notice","turn":0,"createdAt":5,"updatedAt":5,"data":{"text":"Messaged Owen: Hi\\nQueued. Outcome will appear in Owen\'s chat.","heading":"Messaged Owen: Hi","style":"info","status":"queued","delegationId":"d","sourceBotId":"egan","targetBotId":"owen","botMessage":{"sourceBotId":"egan","targetBotId":"owen","text":"Hi"}}}') as Entry
  const out = botExchange(wire)!
  assert.equal(isOutgoing(out), true)
  assert.equal(peerOf(out), 'owen')
  assert.equal(verbOf(out), 'Messaged')
  assert.equal(out.message.text, 'Hi')
  const incoming = botExchange({ ...wire, botId: 'owen', data: { ...wire.data, heading: 'Message from Egan: Hi' } })!
  assert.equal(isOutgoing(incoming), false)
  assert.equal(peerOf(incoming), 'egan')
  assert.equal(verbOf(incoming), 'Message from')
  assert.equal(incoming.message.text, 'Hi')
})

test('structured request is independent of the displayed verb and chat direction', () => {
  for (const intent of ['ask', 'message']) {
    assert.equal(parsed(notice('a', 'egan', 1, 'queued', { intent, request: 'Do it.' })).message.text, 'Do it.')
    assert.equal(parsed(notice('a', 'owen', 1, 'queued', { intent, request: 'Do it.' })).message.text, 'Do it.')
  }
  assert.equal(parsed(notice('a', 'egan', 1, 'queued', { intent: 'ask' })).message.text, 'Inspect this.')
})

test('a name with spaces and a request containing ": " and newlines', () => {
  const heading = 'Asked Owen The Third: Check a: b\nand c: d'
  const x = parsed(notice('a', 'egan', 1, 'queued', { intent: 'ask', heading, request: 'Check a: b\nand c: d' }))
  assert.equal(x.message.text, 'Check a: b\nand c: d')
})

test('completed ask extracts a multi-line reply; message completion has none', () => {
  const ask = parsed(notice('a', 'egan', 1, 'completed', { intent: 'ask', reply: 'Line one.\nLine two: ok.' }))
  assert.equal(ask.message.reply, 'Line one.\nLine two: ok.')
  assert.deepEqual(ask.outcome, { kind: 'done' })
  const msg = parsed(notice('b', 'egan', 2, 'completed', { intent: 'message', outcome: 'Completed.' }))
  assert.equal(msg.message.reply, undefined)
  assert.deepEqual(msg.outcome, { kind: 'done' })
})

test('failed and cancelled carry the outcome as detail', () => {
  const failed = parsed(notice('a', 'egan', 1, 'failed', { outcome: 'Owen stopped.' }))
  assert.deepEqual(failed.outcome, { kind: 'failed', detail: 'Owen stopped.' })
  const cancelled = parsed(notice('b', 'egan', 2, 'cancelled', { intent: 'ask' }))
  assert.deepEqual(cancelled.outcome, { kind: 'failed', detail: '' })
})

test('queued and sent are pending', () => {
  for (const status of ['queued', 'sent']) assert.deepEqual(parsed(notice('a', 'egan', 1, status, { outcome: 'Queued.' })).outcome, { kind: 'pending' })
})

test('structured request does not depend on the rendered notice heading', () => {
  assert.equal(parsed(notice('a', 'egan', 1, 'queued', { heading: 'Something else: odd' })).message.text, 'Inspect this.')
  assert.equal(parsed(notice('a', 'egan', 1, 'queued', { heading: 'Asked Owen: x', intent: 'message' })).message.text, 'Inspect this.')
})

test('existing notices without structured data remain ordinary notices', () => {
  const plain: Entry = { ...notice('a', 'egan', 1, 'sent'), data: { text: 'Hello' } }
  assert.equal(botExchange(plain), null)
  const oldNotice = notice('a', 'egan', 1, 'sent')
  delete oldNotice.data.botMessage
  assert.equal(botExchange(oldNotice), null)
  assert.equal(botExchange({ ...notice('a', 'egan', 1, 'completed'), kind: 'agent' }), null)
})

test('outcome follows the status', () => {
  const of = (status: string, outcome?: string) => botExchange(notice('a', 'egan', 1, status, { outcome }))!.outcome
  assert.deepEqual(of('completed'), { kind: 'done' })
  assert.deepEqual(of('failed', 'Stopped.'), { kind: 'failed', detail: 'Stopped.' })
  assert.deepEqual(of('cancelled'), { kind: 'failed', detail: '' })
  assert.deepEqual(of('queued'), { kind: 'pending' })
  assert.deepEqual(of('sent'), { kind: 'pending' })
})

test('the merge keeps the higher rev, filters the peer and sorts by seq', () => {
  const fetched = [notice('a', 'egan', 2, 'queued', {}, 1000, 1), notice('b', 'egan', 1, 'completed', {}, 900, 1), notice('c', 'egan', 3, 'queued', { targetBotId: 'miles' })]
  const live = [notice('a', 'egan', 2, 'completed', {}, 1000, 2)]
  const merged = botConversation(fetched, live, 'owen')
  assert.deepEqual(merged.map((x) => x.id), ['b', 'a'])
  assert.deepEqual(merged[1]!.outcome, { kind: 'done' })
  const stale = botConversation([notice('a', 'egan', 2, 'completed', {}, 1000, 5)], [notice('a', 'egan', 2, 'queued', {}, 1000, 2)], 'owen')
  assert.deepEqual(stale[0]!.outcome, { kind: 'done' })
})

test('rows: separators, author grouping, reply and status captions', () => {
  const hour = 3_600_000
  const xs = botConversation([
    notice('a', 'egan', 1, 'completed', { intent: 'ask', reply: 'Looks good.' }, 1000),
    notice('b', 'egan', 2, 'completed', {}, 2000),
    notice('c', 'egan', 3, 'queued', {}, 2000 + hour + 1),
    notice('d', 'egan', 4, 'failed', { outcome: 'Stopped.' }, 2000 + hour + 2),
  ], [], 'owen')
  const rows = buildBotConversation(xs)
  assert.deepEqual(rows.map((r) => r.id), [
    'sep-a', 'a-request', 'a-reply', 'b-request', 'sep-c', 'c-request', 'c-status', 'd-request', 'd-status',
  ])
  const shows = (id: string) => (rows.find((r) => r.id === id) as { showsAuthor: boolean }).showsAuthor
  assert.equal(shows('a-request'), true)
  assert.equal(shows('a-reply'), true)
  assert.equal(shows('b-request'), true)
  assert.equal(shows('c-request'), true)
  assert.equal(shows('d-request'), true)
  assert.deepEqual(rows.find((r) => r.id === 'd-status'), { kind: 'status', id: 'd-status', outcome: { kind: 'failed', detail: 'Stopped.' }, awaiting: 'owen' })
})

test('consecutive requests from one author drop the repeated label', () => {
  const rows = buildBotConversation(botConversation([notice('a', 'egan', 1, 'completed'), notice('b', 'egan', 2, 'completed')], [], 'owen'))
  assert.deepEqual(rows.filter((r) => r.kind === 'message').map((r) => (r as { showsAuthor: boolean }).showsAuthor), [true, false])
})

const user = (id: string, seq: number): Entry => ({ ...notice(id, 'egan', seq, 'x'), kind: 'user', data: { text: 'hi' } })
const trace = (id: string, seq: number): Entry => ({ ...notice(id, 'egan', seq, 'x'), kind: 'thought', data: { text: '…' } })
const counts = (es: Entry[]) => groupExchanges(es).map((s) => [s.entry.id, s.exchanges?.length ?? 0])

test('same-peer runs group at their first entry, in either direction', () => {
  const incoming = notice('b', 'egan', 2, 'completed', { sourceBotId: 'owen', targetBotId: 'egan' })
  assert.deepEqual(counts([notice('a', 'egan', 1, 'completed'), incoming, notice('c', 'egan', 3, 'completed')]), [['a', 3]])
})

test('a different peer or a visible entry splits the run', () => {
  const other = notice('b', 'egan', 2, 'completed', { targetBotId: 'miles' })
  assert.deepEqual(counts([notice('a', 'egan', 1, 'completed'), other, notice('c', 'egan', 3, 'completed')]), [['a', 1], ['b', 1], ['c', 1]])
  assert.deepEqual(counts([notice('a', 'egan', 1, 'completed'), user('u', 2), notice('c', 'egan', 3, 'completed')]), [['a', 1], ['u', 0], ['c', 1]])
})

test('trace entries and plain notices: trace is skipped, a plain notice splits', () => {
  assert.deepEqual(counts([notice('a', 'egan', 1, 'completed'), trace('t', 2), notice('c', 'egan', 3, 'completed')]), [['a', 2]])
  const plain: Entry = { ...notice('p', 'egan', 2, 'sent'), data: { text: 'Hello' } }
  assert.deepEqual(counts([notice('a', 'egan', 1, 'completed'), plain, notice('c', 'egan', 3, 'completed')]), [['a', 1], ['p', 0], ['c', 1]])
})

test('the summary counts and flags a failure anywhere in the group', () => {
  const slot = groupExchanges([notice('a', 'egan', 1, 'completed'), notice('b', 'egan', 2, 'failed'), notice('c', 'egan', 3, 'completed')])[0]!
  assert.deepEqual(groupSummary(slot.exchanges!), { peerId: 'owen', count: 3, failed: true })
  assert.equal(groupSummary(groupExchanges([notice('a', 'egan', 1, 'completed')])[0]!.exchanges!).failed, false)
})
