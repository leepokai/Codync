import assert from 'node:assert/strict'
import { test, type TestContext } from 'node:test'
import type { ScreenDisplay } from '@shared/models'
import type { HostClient } from '../client/host-client'

/** A peer that negotiates at once and connects when the test says (`ice`). */
class FakePeer {
  static last: FakePeer | null = null
  iceGatheringState = 'complete'
  iceConnectionState = 'new'
  localDescription: { sdp?: string } | null = null
  oniceconnectionstatechange: (() => void) | null = null
  ontrack: unknown = null
  closed = false
  constructor() {
    FakePeer.last = this
  }
  addTransceiver() {
    return { setCodecPreferences() {} }
  }
  createDataChannel() {
    return { readyState: 'open', send() {}, close() {}, onmessage: null }
  }
  async createOffer() {
    return { type: 'offer', sdp: 'offer' }
  }
  async setLocalDescription(description: { sdp?: string }) {
    this.localDescription = description
  }
  async setRemoteDescription() {}
  addEventListener() {}
  removeEventListener() {}
  close() {
    this.closed = true
  }
  ice(state: string) {
    this.iceConnectionState = state
    this.oniceconnectionstatechange?.()
  }
}
Object.assign(globalThis, { RTCPeerConnection: FakePeer, RTCRtpReceiver: { getCapabilities: () => ({ codecs: [] }) } })
const { ScreenSession } = await import('./screen-session.ts')

const display = { id: 0, name: 'Display 1', width: 1920, height: 1080, main: true } as ScreenDisplay

/** A host that hands out sessions `s1`, `s2`, … valid for `validMs`, until `refuse` says otherwise. */
function host(validMs = 3_600_000) {
  const calls: [string, unknown][] = []
  let prepared = 0
  const state = {
    calls,
    refuse: null as Error | null,
    /** Holds the next `screenPrepare` until the test answers it. */
    hold: null as ((answer: () => void) => void) | null,
    methods: () => calls.map(([method, body]) => (method === 'screenClose' ? `close ${(body as { session: string }).session}` : method)),
  }
  const client = {
    call: async (method: string, body: unknown) => {
      calls.push([method, body])
      if (method === 'screenPrepare') {
        if (state.refuse) throw state.refuse
        if (state.hold) await new Promise<void>((resolve) => state.hold!(resolve))
        return { session: `s${++prepared}`, iceServers: [], expiresAt: Date.now() + validMs }
      }
      return method === 'screenOffer' ? { sdp: 'answer' } : {}
    },
  } as unknown as HostClient
  return { client, state }
}

/** Lets every pending promise run (the fakes answer in microtasks). */
const settle = () => new Promise((resolve) => setImmediate(resolve))

async function advance(t: TestContext, ms: number) {
  t.mock.timers.tick(ms)
  await settle()
}

test('a renewal reserves the next session before closing the old one, so a takeover survives', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  // Credentials valid for 5 min 1 s: the renewal comes after 1 s.
  const { client, state } = host(301_000)
  const session = new ScreenSession(client, true, display)
  await session.start()
  FakePeer.last!.ice('connected')
  assert.equal(session.phase.kind, 'live')
  await advance(t, 1000)
  await advance(t, 0)
  assert.deepEqual(state.methods(), ['screenPrepare', 'screenOffer', 'screenPrepare', 'close s1', 'screenOffer'])
  FakePeer.last!.ice('connected')
  assert.equal(session.phase.kind, 'live')
  session.close()
})

test('recovery gives up after three attempts with the last error', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const { client, state } = host()
  const session = new ScreenSession(client, false, display)
  await session.start()
  FakePeer.last!.ice('connected')
  state.refuse = new Error('The computer went away.')
  FakePeer.last!.ice('failed')
  assert.equal(session.phase.kind, 'reconnecting')
  for (const wait of [0, 2000, 4000, 6000]) await advance(t, wait)
  assert.deepEqual(session.phase, { kind: 'failed', message: 'The computer went away.' })
  assert.equal(state.methods().filter((m) => m === 'screenPrepare').length, 4)
})

test('a close while preparing ends quietly and gives the reserved session back', async () => {
  const { client, state } = host()
  let answer = () => {}
  state.hold = (resolve) => (answer = resolve)
  const session = new ScreenSession(client, false, display)
  const started = session.start()
  await settle()
  session.close()
  answer()
  await started
  assert.equal(session.phase.kind, 'closed')
  assert.deepEqual(state.methods(), ['screenPrepare', 'close s1'])
})

test('a peer that never connects fails after the connect timeout', async (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const { client, state } = host()
  const session = new ScreenSession(client, false, display)
  await session.start()
  assert.equal(session.phase.kind, 'connecting')
  await advance(t, 30_000)
  assert.equal(session.phase.kind, 'failed')
  assert.equal(FakePeer.last!.closed, true)
  assert.deepEqual(state.methods().at(-1), 'close s1')
})
