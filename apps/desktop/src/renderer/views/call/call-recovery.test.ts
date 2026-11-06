import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'

// Recorded Gemini Live and OpenAI Realtime event streams replayed through the engines with a scripted socket, microphone
// and speaker (kit CallRecoveryTests): phases, cues, resumption, the #214 shapes and failure.

// The renderer imports without extensions, as the bundler resolves them.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
      const url = new URL(`${specifier}.ts`, context.parentURL)
      if (existsSync(url)) return nextResolve(url.href, context)
    }
    return nextResolve(specifier, context)
  },
})

const disk = new Map<string, string>()
Object.assign(globalThis, {
  localStorage: { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => disk.set(key, value) },
  window: { addEventListener() {} },
})
const { GeminiLiveEngine } = await import('./GeminiLiveEngine.ts')
const { OpenAIRealtimeEngine } = await import('./OpenAIRealtimeEngine.ts')
const { CallQuality } = await import('./call-quality.ts')
const { reconnect } = await import('./voice.ts')
reconnect.delays = [5, 5, 5]
GeminiLiveEngine.stallMs = 40

class FakeSocket {
  readyState = 0
  onopen: (() => void) | null = null
  onmessage: ((e: { data: unknown }) => void) | null = null
  onclose: ((e: { code: number; reason: string }) => void) | null = null
  sent: Record<string, any>[] = []
  open() {
    this.readyState = 1
    this.onopen?.()
  }
  play(message: unknown) {
    this.onmessage?.({ data: JSON.stringify(message) })
  }
  drop(reason = '', code = 1006) {
    if (this.readyState === 3) return
    this.readyState = 3
    this.onclose?.({ code, reason })
  }
  send(data: string) {
    this.sent.push(JSON.parse(data) as Record<string, any>)
  }
  close(code = 1000) {
    this.drop('', code)
  }
  of(key: string) {
    return this.sent.filter((m) => key in m)
  }
}

const until = async (condition: () => boolean, ms = 2000) => {
  for (let waited = 0; waited < ms; waited += 5) {
    if (condition()) return true
    await new Promise((resolve) => setTimeout(resolve, 5))
  }
  return condition()
}

function call() {
  const sockets: FakeSocket[] = []
  const plays: (() => void)[] = []
  let mic: ((pcm: Int16Array, level: number) => void) | null = null
  let starts = 0
  const link = {
    start: async () => {
      starts++
      return { credential: `token-${starts}`, instructions: 'Be the relay.', tools: [] }
    },
    add() {},
    endTurn() {},
    transcribing() {},
    tool: async () => ({}),
    reply: (text: string) => `[Bot replied] ${text}`,
    notice: (text: string) => `[Notice] ${text}`,
    progress: (text: string) => `[Progress] ${text}`,
  }
  const ports = {
    socket: () => {
      const socket = new FakeSocket()
      sockets.push(socket)
      return socket
    },
    mic: async (out: (pcm: Int16Array, level: number) => void) => {
      mic = out
      return () => {
        mic = null
      }
    },
    output: () => ({
      play: (_pcm: Uint8Array, done: () => void) => void plays.push(done),
      stop: () => void plays.splice(0),
      close() {},
    }),
  }
  const engine = new GeminiLiveEngine('gemini-live', 'Kore', link as never, ports)
  return {
    engine,
    sockets,
    plays,
    get starts() {
      return starts
    },
    get micOn() {
      return mic !== null
    },
    deliver: () => mic?.(new Int16Array(320), 0.5),
    last: () => sockets[sockets.length - 1]!,
  }
}

/** Started, set up, and the microphone delivering. */
async function ready(c: ReturnType<typeof call>) {
  void c.engine.start()
  assert.ok(await until(() => c.sockets.length === 1))
  c.last().open()
  c.last().play({ setupComplete: {} })
  assert.ok(await until(() => c.micOn))
  c.deliver()
  assert.equal(c.engine.phase, 'listening')
}

test('Gemini is ready only once the microphone delivers after setup', async () => {
  const c = call()
  void c.engine.start()
  assert.ok(await until(() => c.sockets.length === 1))
  c.last().open()
  assert.equal(c.last().of('setup').length, 1)
  c.last().play({ setupComplete: {} })
  assert.ok(await until(() => c.micOn))
  assert.equal(c.engine.phase, 'starting', 'setup alone is not ready')
  c.deliver()
  assert.equal(c.engine.phase, 'listening')
  assert.equal(c.last().of('realtimeInput').length, 1)
  c.engine.end()
})

test('a dropped Gemini connection resumes without hanging up', async () => {
  const c = call()
  await ready(c)
  c.last().play({ sessionResumptionUpdate: { newHandle: 'h1', resumable: true } })
  c.last().drop('network')
  assert.equal(c.engine.phase, 'reconnecting')
  assert.ok(c.engine.holdsCall, 'the call is not over')
  c.engine.speak('The tests pass.')
  assert.ok(await until(() => c.sockets.length === 2))
  assert.equal(c.starts, 2, 'a fresh credential from the computer')
  c.last().open()
  assert.equal(c.last().of('setup')[0]!.setup.sessionResumption.handle, 'h1')
  c.last().play({ setupComplete: {} })
  assert.equal(c.last().of('clientContent').length, 1, 'the reply that waited is said')
  assert.equal(c.engine.phase, 'reconnecting', 'ready again once the microphone delivers')
  c.deliver()
  assert.equal(c.engine.phase, 'listening')
  c.engine.end()
})

test('goAway carries on in a resumed session without reconnecting', async () => {
  const c = call()
  await ready(c)
  c.last().play({ sessionResumptionUpdate: { newHandle: 'h2', resumable: true } })
  c.last().play({ goAway: { timeLeft: '5s' } })
  assert.ok(await until(() => c.sockets.length === 2))
  assert.equal(c.engine.phase, 'listening')
  c.engine.end()
})

test('goAway mid-turn does not leave the call speaking', async () => {
  const c = call()
  await ready(c)
  c.last().play({ serverContent: { modelTurn: { parts: [{ inlineData: { data: 'AAAAAA==' } }] } } })
  assert.equal(c.engine.phase, 'speaking')
  c.last().play({ goAway: { timeLeft: '5s' } })
  assert.ok(await until(() => c.sockets.length === 2))
  c.plays.splice(0).forEach((done) => done())
  assert.equal(c.engine.phase, 'listening', 'the resumed session will not complete that turn')
  c.engine.speak('Deployed.')
  c.last().open()
  c.last().play({ setupComplete: {} })
  assert.equal(c.last().of('clientContent').length, 1, 'replies are not held behind it')
  c.engine.end()
})

test('a session Gemini refuses mid-call fails at once', async () => {
  const c = call()
  await ready(c)
  c.last().drop('API key not valid.', 1008)
  assert.equal(c.engine.phase, 'failed')
  assert.equal(c.engine.failureKind, 'provider', 'trying again will not help')
  assert.equal(c.engine.failure, 'API key not valid.')
  assert.equal(c.sockets.length, 1)
})

test('reconnects run out and the call fails', async () => {
  const c = call()
  await ready(c)
  for (let attempt = 1; attempt <= reconnect.delays.length; attempt++) {
    c.last().drop()
    assert.ok(await until(() => c.sockets.length === attempt + 1))
  }
  c.last().drop()
  assert.equal(c.engine.phase, 'failed')
  assert.equal(c.engine.failureKind, 'connection')
})

test('playback that never finishes goes back to listening (#214)', async () => {
  const c = call()
  let stalls = 0
  c.engine.onStall = () => stalls++
  await ready(c)
  c.last().play({ serverContent: { modelTurn: { parts: [{ inlineData: { data: 'AAAAAA==' } }] } } })
  assert.equal(c.engine.phase, 'speaking')
  c.last().play({ serverContent: { turnComplete: true } })
  c.engine.speak('Deployed.')
  assert.ok(await until(() => c.engine.phase === 'listening'))
  assert.equal(stalls, 1)
  assert.equal(c.last().of('clientContent').length, 1, 'the reply is not stuck behind it')
  c.engine.end()
})

test('playback that finishes returns to listening without a stall', async () => {
  const c = call()
  let stalls = 0
  c.engine.onStall = () => stalls++
  await ready(c)
  c.last().play({ serverContent: { modelTurn: { parts: [{ inlineData: { data: 'AAAAAA==' } }] } } })
  c.last().play({ serverContent: { turnComplete: true } })
  c.plays.splice(0).forEach((done) => done())
  assert.equal(c.engine.phase, 'listening')
  await new Promise((resolve) => setTimeout(resolve, 100))
  assert.equal(stalls, 0)
  c.engine.end()
})

test('a changed microphone reconnects until it delivers again', async () => {
  const c = call()
  await ready(c)
  await (c.engine as unknown as { restartMic(): Promise<void> }).restartMic()
  assert.equal(c.engine.phase, 'reconnecting')
  c.deliver()
  assert.equal(c.engine.phase, 'listening')
  c.engine.end()
})

test('OpenAI phases follow its events', () => {
  const engine = new OpenAIRealtimeEngine('gpt-realtime', 'marin', {} as never)
  const event = (type: string) => (engine as unknown as { received(data: string): void }).received(JSON.stringify({ type }))
  event('output_audio_buffer.started')
  assert.equal(engine.phase, 'starting', 'nothing plays before the session is set up')
  event('session.updated')
  assert.equal(engine.phase, 'listening')
  event('output_audio_buffer.started')
  assert.equal(engine.phase, 'speaking')
  event('output_audio_buffer.stopped')
  assert.equal(engine.phase, 'listening')
  engine.end()
})

/** OpenAI's side of a call: a WebRTC peer whose events channel and ICE state the test drives. */
class FakePeer {
  iceConnectionState = 'new'
  oniceconnectionstatechange: (() => void) | null = null
  ontrack: unknown = null
  answered = false
  channel = {
    readyState: 'connecting',
    onopen: null as (() => void) | null,
    onmessage: null as ((e: { data: unknown }) => void) | null,
    onclose: null as (() => void) | null,
    send() {},
    close() {
      this.readyState = 'closed'
    },
  }
  addTrack() {}
  createDataChannel() {
    return this.channel
  }
  async createOffer() {
    return { type: 'offer', sdp: 'offer-sdp' }
  }
  async setLocalDescription() {}
  async setRemoteDescription() {
    this.answered = true
  }
  getSenders() {
    return []
  }
  close() {}
  /** The session is set up. */
  ready() {
    this.channel.readyState = 'open'
    this.channel.onopen?.()
    this.channel.onmessage?.({ data: JSON.stringify({ type: 'session.updated' }) })
  }
  ice(state: string) {
    this.iceConnectionState = state
    this.oniceconnectionstatechange?.()
  }
}

function openAICall() {
  const peers: FakePeer[] = []
  const answers = { status: 200 }
  let starts = 0
  const track = { kind: 'audio', enabled: true, stop() {} }
  Object.defineProperty(globalThis, 'navigator', {
    configurable: true,
    value: {
      mediaDevices: { getUserMedia: async () => ({ getAudioTracks: () => [track], getTracks: () => [track] }), addEventListener() {}, removeEventListener() {} },
    },
  })
  Object.assign(globalThis, {
    RTCPeerConnection: class extends FakePeer {
      constructor() {
        super()
        peers.push(this)
      }
    },
    Audio: class {},
    fetch: async () => ({ ok: answers.status === 200, status: answers.status, text: async () => 'answer-sdp' }),
  })
  const link = {
    start: async () => {
      starts++
      return { credential: `ek-${starts}`, instructions: 'Be the relay.', tools: [] }
    },
  }
  const engine = new OpenAIRealtimeEngine('gpt-realtime', 'marin', link as never)
  return {
    engine,
    answers,
    get starts() {
      return starts
    },
    last: () => peers[peers.length - 1]!,
    peers,
  }
}

test('OpenAI reconnects a dropped session and gives up after three tries', async () => {
  const o = openAICall()
  void o.engine.start()
  assert.ok(await until(() => o.peers.length === 1 && o.last().answered))
  o.last().ready()
  assert.equal(o.engine.phase, 'listening')
  o.last().ice('failed')
  assert.equal(o.engine.phase, 'reconnecting', 'tried again, not hung up')
  assert.ok(await until(() => o.peers.length === 2 && o.last().answered))
  assert.equal(o.starts, 2, 'a fresh secret and instructions from the computer')
  o.last().ready()
  assert.equal(o.engine.phase, 'listening')
  // OpenAI stops answering: three more tries (the count started over once the session was back), then the call fails.
  o.answers.status = 503
  o.last().ice('failed')
  assert.ok(await until(() => o.engine.phase === 'failed'))
  assert.equal(o.engine.failureKind, 'connection')
  assert.equal(o.starts, 2 + reconnect.delays.length)
})

test('OpenAI that never connected fails over at once', async () => {
  const o = openAICall()
  o.answers.status = 401
  await o.engine.start()
  assert.equal(o.engine.phase, 'failed')
  assert.equal(o.engine.failureKind, 'provider')
  assert.equal(o.starts, 1)
})

test('call quality reports ready time, reconnects, stalls and failure once per engine', () => {
  const events: unknown[] = []
  const quality = new CallQuality((e) => events.push(e))
  quality.begin('gemini')
  quality.phase('listening', null)
  quality.phase('reconnecting', null)
  quality.phase('listening', null)
  quality.stalled()
  quality.phase('failed', 'connection')
  quality.begin('on_device')
  const first = events[0] as Record<string, unknown>
  assert.equal(first.event, 'voice_call_quality')
  assert.equal(first.engine, 'gemini')
  assert.equal(typeof first.ready_ms, 'number')
  assert.deepEqual([first.reconnects, first.stuck, first.failure], [1, 'speaking', 'connection'])
  quality.report()
  assert.equal(events.length, 2)

  let now = 0
  const stuck: unknown[] = []
  const slow = new CallQuality((e) => stuck.push(e), () => now)
  slow.begin('openai')
  now += CallQuality.stuckMs
  slow.report()
  assert.deepEqual(stuck, [{ event: 'voice_call_quality', engine: 'openai', reconnects: 0, stuck: 'connecting', failure: 'none' }])
})
