import assert from 'node:assert/strict'
import { test } from 'node:test'
import {
  decodeReply, negotiate, preferredCodecs, renewIn, screenRelay, Superseded, viewRequest,
  type Negotiation, type ScreenConnection,
} from './screen-signaling.ts'

const connection: ScreenConnection = { session: 's1', iceServers: [], expiresAt: 0, trickle: false }

/** A negotiation that logs its steps; `gathering` holds the offer's SDP back until released. */
function fake(options: { current?: () => boolean } = {}) {
  const log: string[] = []
  let release: () => void = () => {}
  const gathering = new Promise<void>((resolve) => (release = resolve))
  const n: Negotiation = {
    async prepare(relay) { log.push(`prepare relay=${relay}`); return connection },
    start(c) { log.push(`start ${c.session}`) },
    async offerSdp() { log.push('offer created'); await gathering; log.push('gathered'); return 'sdp-with-candidates' },
    async offer(session, sdp) { log.push(`offer ${session} ${sdp}`); return { session: 's2', sdp: 'answer' } },
    async accept(a) { log.push(`accept ${a.sdp}`) },
    current: options.current ?? (() => true),
  }
  return { n, log, release }
}

test('prepare, then the offer only once ICE gathering is done, then the answer', async () => {
  const { n, log, release } = fake()
  const done = negotiate(n, false)
  await new Promise((resolve) => setImmediate(resolve))
  assert.deepEqual(log, ['prepare relay=false', 'start s1', 'offer created'], 'nothing is offered while gathering')
  release()
  const result = await done
  assert.deepEqual(log.slice(3), ['gathered', 'offer s1 sdp-with-candidates', 'accept answer'])
  assert.equal(result.session, 's2', 'the answer names the session')
})

test('a newer connection or a close stops the negotiation between steps', async () => {
  let alive = true
  const { n, log, release } = fake({ current: () => alive })
  const done = negotiate(n, false)
  await new Promise((resolve) => setImmediate(resolve))
  alive = false
  release()
  await assert.rejects(done, Superseded)
  assert.ok(!log.some((line) => line.startsWith('offer s1')), 'a replaced connection never offers')
})

test('relay follows the host channel: only the cloud one', () => {
  assert.equal(screenRelay('relay'), true)
  assert.equal(screenRelay('loopback'), false, 'an SSH tunnel or this computer')
  assert.equal(screenRelay('direct'), false, 'LAN')
  assert.equal(screenRelay(null), false)
})

test('credentials renew five minutes early, never sooner than a second', () => {
  assert.equal(renewIn(1_000_000, 0), 700_000)
  assert.equal(renewIn(1_000_000, 900_000), 1_000)
  assert.equal(renewIn(0, 5_000), 1_000)
})

test('replies are decoded; unknown or broken messages are ignored without throwing', () => {
  assert.deepEqual(decodeReply('{"type":"clipboard","text":"hi"}'), { type: 'clipboard', text: 'hi' })
  assert.deepEqual(decodeReply('{"type":"clipboard"}'), { type: 'clipboard', text: null })
  assert.deepEqual(decodeReply('{"type":"view","crop":[0,0,10,10]}'), { type: 'view', crop: [0, 0, 10, 10] })
  assert.deepEqual(decodeReply('{"type":"view","crop":[0,0]}'), { type: 'view', crop: null })
  assert.deepEqual(decodeReply('{"type":"error","message":"no"}'), { type: 'error', message: 'no' })
  for (const junk of ['{"type":"telepathy"}', 'not json', '', 'null', '7', '[]', '{"type":5}']) assert.equal(decodeReply(junk), null, junk)
})

test('the view request carries the long and short side in pixels', () => {
  assert.deepEqual(viewRequest(null, { width: 1600, height: 900 }), { type: 'view', long: 1600, short: 900 })
  assert.deepEqual(viewRequest(3, { width: 900, height: 1600 }), { type: 'view', long: 1600, short: 900, display: 3 })
})

test('H.264 comes first, High before the rest, other codecs last', () => {
  const codecs = [
    { mimeType: 'video/VP8' },
    { mimeType: 'video/H264', sdpFmtpLine: 'profile-level-id=42e01f' },
    { mimeType: 'video/AV1' },
    { mimeType: 'video/H264', sdpFmtpLine: 'profile-level-id=640c1f' },
  ]
  assert.deepEqual(preferredCodecs(codecs).map((c) => c.sdpFmtpLine ?? c.mimeType), ['profile-level-id=640c1f', 'profile-level-id=42e01f', 'video/VP8', 'video/AV1'])
})
