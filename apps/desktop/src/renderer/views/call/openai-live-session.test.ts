import assert from 'node:assert/strict'
import { test } from 'node:test'
import { OpenAILiveSession, gatheredOffer, isLiveModel } from './openai-live-session.ts'

const tick = () => new Promise<void>((resolve) => setImmediate(resolve))

function fixture() {
  const sent: unknown[] = []
  const seen: unknown[] = []
  const failures: string[] = []
  const link = {
    startLive: async (_model: string, _voice: string, sdp: string) => {
      seen.push(sdp)
      return { sessionId: 'live-one', sdp: 'answer' }
    },
    liveEvent: async (id: string, event: unknown): Promise<{ events: unknown[] }> => {
      seen.push({ id, event })
      return { events: [{ type: 'response.item.create' }] }
    },
    liveUpdate: async (id: string, kind: string, text: string) => {
      seen.push({ id, kind, text })
      return { events: [{ type: 'session.commentary.append', content: text }] }
    },
  }
  const session = new OpenAILiveSession(link, (event) => sent.push(event), (error) => failures.push(error))
  return { session, link, sent, seen, failures }
}

test('Live excludes transcription and Realtime models', () => {
  assert.equal(isLiveModel('gpt-live-1'), true)
  assert.equal(isLiveModel('gpt-live-1.1'), true)
  assert.equal(isLiveModel('gpt-live-transcribe'), false)
  assert.equal(isLiveModel('gpt-realtime-2.1'), false)
})

test('Live preserves host event order and delegates only supported event types', async () => {
  const c = fixture()
  assert.equal(await c.session.start('gpt-live-1', 'marin', 'offer'), 'answer')
  c.session.receive({ type: 'session.input_transcript.delta', delta: 'hello' })
  c.session.receive({ type: 'response.event', event: { type: 'response.output_item.done' } })
  c.session.receive({ type: 'session.output_audio.delta', delta: 'not-control-data' })
  c.session.update('reply', 'Result')
  await tick()
  assert.equal(c.seen.length, 4)
  assert.deepEqual(c.seen[3], { id: 'live-one', kind: 'reply', text: 'Result' })
  assert.equal(c.sent.length, 3)
  assert.deepEqual(c.failures, [])
})

test('replaced Live sessions never send late host tool results', async () => {
  const c = fixture()
  let complete: ((v: { events: unknown[] }) => void) | undefined
  c.link.liveEvent = () => new Promise((resolve) => { complete = resolve })
  await c.session.start('gpt-live-1', 'marin', 'offer')
  c.session.receive({ type: 'response.event' })
  await tick()
  c.session.discard()
  complete?.({ events: [{ type: 'response.create' }] })
  await tick()
  assert.deepEqual(c.sent, [])
})

test('Live host failures surface and do not retry side effects', async () => {
  const c = fixture()
  let calls = 0
  c.link.liveEvent = async () => { calls++; throw new Error('Host went offline') }
  await c.session.start('gpt-live-1', 'marin', 'offer')
  c.session.receive({ type: 'response.event' })
  c.session.receive({ type: 'response.event' })
  await tick()
  assert.equal(calls, 1)
  assert.deepEqual(c.failures, ['Host went offline'])
})

test('Live shutdown waits for acknowledgement and drops queued results', async () => {
  const c = fixture()
  await c.session.start('gpt-live-1', 'marin', 'offer')
  const closing = c.session.shutdown()
  assert.deepEqual(c.sent, [{ type: 'session.close' }])
  c.session.receive({ type: 'session.closed' })
  await closing
  c.session.update('reply', 'late')
  await tick()
  assert.equal(c.sent.length, 1)
  assert.deepEqual(c.failures, [])
})

test('Live uses the gathered local SDP instead of an incomplete initial offer', async () => {
  const pc = { iceGatheringState: 'complete', localDescription: { sdp: 'offer-with-candidates' } }
  assert.equal(await gatheredOffer(pc as RTCPeerConnection), 'offer-with-candidates')
  await assert.rejects(gatheredOffer({ iceGatheringState: 'complete' } as RTCPeerConnection), /no SDP/)
})
