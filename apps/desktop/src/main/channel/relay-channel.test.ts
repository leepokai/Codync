import assert from 'node:assert/strict'
import { createHash, createPublicKey, diffieHellman, generateKeyPairSync, hkdfSync, sign } from 'node:crypto'
import { mock, test } from 'node:test'
import { b64url, computerIdRaw, FrameCodec, privateKey, rawPublic, unb64url } from './crypto.ts'
import { RelayChannel, type Socket } from './relay-channel.ts'

const hostKey = privateKey('ed25519', Buffer.alloc(32, 1))
const hostPub = rawPublic(hostKey)
const deviceKey = privateKey('ed25519', Buffer.alloc(32, 3))
const device = { key: deviceKey, pub: rawPublic(deviceKey) }

type Listener = (event: { data?: unknown; code?: number }) => void

/** The relay and the computer behind it, as one socket. */
class FakeRelay implements Socket {
  sent: Record<string, unknown>[] = []
  closedWith: number | null = null
  codec: FrameCodec | null = null
  private listeners = new Map<string, Listener[]>()

  addEventListener(type: string, listener: Listener) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener])
  }
  send(text: string) {
    const wire = JSON.parse(text) as Record<string, unknown>
    if (wire.t === 'hello') return this.welcome(wire as { dk: string; ek: string; n: string })
    this.sent.push(wire.t === 'f' && this.codec ? (this.codec.open(wire as { c: number; d: string }) as Record<string, unknown>) : wire)
  }
  close(code = 1000) {
    // As the real WebSocket does.
    if (code !== 1000 && (code < 3000 || code > 4999)) throw new Error(`InvalidAccessError: invalid code ${code}`)
    this.closedWith = code
    this.emit('close', { code })
  }
  emit(type: string, event: { data?: unknown; code?: number }) {
    for (const l of this.listeners.get(type) ?? []) l(event)
  }
  push(message: unknown) {
    this.emit('message', { data: JSON.stringify(message) })
  }
  /** An inner message from the computer. */
  answer(inner: unknown) {
    for (const f of this.codec!.seal(inner)) this.push(f)
  }
  private welcome(hello: { dk: string; ek: string; n: string }) {
    const eph = generateKeyPairSync('x25519').privateKey
    const ekH = rawPublic(eph)
    const th = createHash('sha256').update(Buffer.concat([Buffer.from('codync/hs2/v1'), computerIdRaw(hostPub), unb64url(hello.dk), unb64url(hello.ek), unb64url(hello.n), ekH])).digest()
    const ss = diffieHellman({ privateKey: eph, publicKey: publicX25519(unb64url(hello.ek)) })
    const key = (info: string) => Buffer.from(hkdfSync('sha256', ss, th, info, 32))
    // The computer seals with h2d and opens with d2h.
    this.codec = new FrameCodec({ d2h: key('codync/h2d/v1'), h2d: key('codync/d2h/v1') })
    this.push({ t: 'welcome', v: 1, ek: b64url(ekH), sig: b64url(sign(null, th, hostKey)) })
  }
}

const publicX25519 = (raw: Buffer) => createPublicKey({ key: { kty: 'OKP', crv: 'X25519', x: b64url(raw) }, format: 'jwk' })

function channel() {
  const relays: FakeRelay[] = []
  let refused = 0
  const ch = new RelayChannel({
    computerId: b64url(computerIdRaw(hostPub)),
    signKey: b64url(hostPub),
    device,
    dial: () => {
      const r = new FakeRelay()
      relays.push(r)
      return r
    },
    onRefused: () => void refused++,
  })
  ch.setAttended(true)
  return { ch, relays, refused: () => refused }
}

const tick = () => new Promise((r) => setImmediate(r))

test('a call waits while the computer sleeps, then goes out once it is back', async () => {
  const { ch, relays } = channel()
  const answer = ch.call('hello', {}, 5000)
  const relay = relays[0]!
  relay.push({ t: 'presence', online: false, lastSeenAt: 1 })
  assert.equal(ch.state, 'asleep')
  relay.push({ t: 'presence', online: true })
  await tick()
  assert.equal(ch.state, 'ready')
  assert.deepEqual(relay.sent.at(-1), { id: 1, m: 'hello', b: {} })
  relay.answer({ id: 1, ok: { name: 'Codync Cloud' } })
  assert.deepEqual(await answer, { name: 'Codync Cloud' })
  ch.close()
})

test('a subscription gets its events and ends when the socket drops; a sleeping computer times out as offline', async () => {
  const { ch, relays } = channel()
  const events: unknown[] = []
  let ended: unknown = 'open'
  ch.subscribe('events', { since: 4, client: 'desktop' }, (e) => events.push(e), (e) => (ended = e))
  const relay = relays[0]!
  relay.push({ t: 'presence', online: true })
  await tick()
  assert.deepEqual(relay.sent.at(-1), { id: 1, sub: 'events', b: { since: 4, client: 'desktop' } })
  relay.answer({ id: 1, ev: { type: 'hello', rev: 4 } })
  assert.deepEqual(events, [{ type: 'hello', rev: 4 }])
  relay.push({ t: 'presence', online: false, lastSeenAt: 2 })
  assert.deepEqual(ended, { status: 0, message: 'Your computer is offline.', offline: true })
  await assert.rejects(ch.call('hello', {}, 20), { offline: true })
  ch.close()
})

test('a computer that turns this device away stops the channel for good', async () => {
  const { ch, relays, refused } = channel()
  const answer = ch.call('hello', {}, 5000)
  relays[0]!.push({ t: 'presence', online: true })
  relays[0]!.push({ t: 'reject', code: 'revoked' })
  await assert.rejects(answer, { status: 401 })
  assert.equal(refused(), 1)
  await assert.rejects(ch.call('hello', {}, 5000), { status: 401 })
  assert.equal(relays.length, 1)
})

test('nobody looking at the window closes the socket; calls wait until someone looks again', async () => {
  const { ch, relays } = channel()
  const first = ch.call('hello', {}, 5000)
  relays[0]!.push({ t: 'presence', online: true })
  await tick()
  relays[0]!.answer({ id: 1, ok: {} })
  await first
  ch.setAttended(false)
  assert.equal(relays[0]!.closedWith, 1000)
  const waiting = ch.call('routines', {}, 5000)
  await tick()
  assert.equal(relays.length, 1)
  ch.setAttended(true)
  assert.equal(relays.length, 2)
  relays[1]!.push({ t: 'presence', online: true })
  await tick()
  relays[1]!.answer({ id: 2, ok: { routines: [] } })
  assert.deepEqual(await waiting, { routines: [] })
  ch.close()
})

test('a socket that goes quiet or fails is closed and dialed again', async () => {
  mock.timers.enable({ apis: ['setInterval', 'setTimeout', 'Date'] })
  try {
    const { ch, relays } = channel()
    const answer = ch.call('hello', {}, 600_000)
    relays[0]!.emit('error', {})
    assert.equal(relays[0]!.closedWith, 4000)
    mock.timers.tick(31_000)
    assert.equal(relays.length, 2)
    relays[1]!.push({ t: 'presence', online: true })
    mock.timers.tick(120_000)
    assert.equal(relays[1]!.closedWith, 4000)
    ch.close()
    await assert.rejects(answer)
  } finally {
    mock.timers.reset()
  }
})
