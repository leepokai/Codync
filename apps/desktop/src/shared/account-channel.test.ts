import assert from 'node:assert/strict'
import * as crypto from 'node:crypto'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import * as channelCrypto from '../main/channel-crypto.ts'
import { loadMainModule, settle } from './test-support/main-module.ts'
import type * as AccountChannelModule from '../main/account-channel.ts'

const { FrameOpener, FrameSealer, computerId, signingKey } = channelCrypto
const v = JSON.parse(readFileSync(new URL('../../../../docs/reference/fixtures/remote-relay-vectors.json', import.meta.url), 'utf8'))
const hostSeed = Buffer.from(v.keys.hostSignSeed, 'base64url')
const hostKey = v.keys.hostSignPub as string
const computer = { id: computerId(hostKey), signKey: hostKey }
const device = signingKey(Buffer.from(v.keys.deviceSignSeed, 'base64url'))

/** The relay's socket as the channel sees it; the test plays the relay and the host. */
class FakeSocket {
  static OPEN = 1
  static last: FakeSocket | null = null
  readyState = 1
  sent: Record<string, unknown>[] = []
  url: string
  headers: Record<string, string>
  onmessage: ((e: { data: string }) => void) | null = null
  onclose: ((e: { code: number; reason: string }) => void) | null = null
  onerror: (() => void) | null = null
  constructor(url: string, init: { headers: Record<string, string> }) {
    this.url = url
    this.headers = init.headers
    FakeSocket.last = this
  }
  send(text: string) {
    this.sent.push(JSON.parse(text))
  }
  close() {
    this.readyState = 3
  }
  receive(message: unknown) {
    this.onmessage?.({ data: JSON.stringify(message) })
  }
}

/** The host's half of the handshake (host `remote::channel`): welcome signed by `signer`, then frames both ways. */
function host(socket: FakeSocket, signer = hostSeed) {
  const hello = socket.sent.find((m) => m.t === 'hello') as { dk: string; ek: string; n: string }
  const raw = (s: string) => Buffer.from(s, 'base64url')
  const ephemeral = crypto.generateKeyPairSync('x25519')
  const ekH = (ephemeral.publicKey.export({ format: 'jwk' }) as { x: string }).x
  const cid = raw(computer.id)
  const transcript = crypto.createHash('sha256').update(Buffer.concat([Buffer.from('codync/hs2/v1'), cid, raw(hello.dk), raw(hello.ek), raw(hello.n), raw(ekH)])).digest()
  const signKey = crypto.createPrivateKey({ key: Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), signer]), format: 'der', type: 'pkcs8' })
  const peer = crypto.createPublicKey({ key: { kty: 'OKP', crv: 'X25519', x: hello.ek }, format: 'jwk' })
  const shared = crypto.diffieHellman({ privateKey: ephemeral.privateKey, publicKey: peer })
  const derive = (info: string) => Buffer.from(crypto.hkdfSync('sha256', shared, transcript, info, 32))
  socket.receive({ t: 'welcome', ek: ekH, sig: crypto.sign(null, transcript, signKey).toString('base64url') })
  const opener = new FrameOpener(derive('codync/d2h/v1'))
  const sealer = new FrameSealer(derive('codync/h2d/v1'))
  let read = 0
  return {
    /** What the device sent since the last look, decrypted. */
    inbox() {
      const frames = socket.sent.slice(read).filter((m) => m.t === 'f') as { c: number; d: string }[]
      read = socket.sent.length
      return frames.flatMap((f) => {
        const message = opener.open(f.c, f.d)
        return message ? [JSON.parse(message.toString('utf8'))] : []
      })
    },
    reply(message: unknown) {
      for (const frame of sealer.seal(Buffer.from(JSON.stringify(message)))) socket.receive({ t: 'f', ...frame })
    },
  }
}

type CallResult = { ok: true; value: unknown } | { ok: false; error: { status: number; message: string } }
type Sender = { isDestroyed(): boolean; send(...args: unknown[]): void }
type Handlers = {
  'remote:call': (e: object, c: typeof computer, method: string, body: unknown, timeoutMs: number) => Promise<CallResult>
  'remote:stream:open': (e: { sender: Sender }, id: string, c: typeof computer, kind: string, body: unknown) => void
  'remote:close': (e: object, computerId: string) => void
}
const handlers = new Map<string, Function>()
const handler = <K extends keyof Handlers>(name: K) => handlers.get(name) as Handlers[K]
const ipcMain = { handle: (name: string, fn: Function) => handlers.set(name, fn), on: (name: string, fn: Function) => handlers.set(name, fn) }
const { registerAccountChannels, closeAccountChannels } = loadMainModule<typeof AccountChannelModule>('account-channel', {
  electron: { ipcMain },
  './channel-crypto': channelCrypto,
}, { WebSocket: FakeSocket, URL, Buffer, JSON, setTimeout, clearTimeout, setInterval, clearInterval })
registerAccountChannels({ relay: () => 'https://cloud.test', device: async () => device, sign: async (url: URL) => `sig for ${url.pathname}` })

/** Starts a call and returns the socket it opened, past `presence`. */
async function connect(method: string) {
  FakeSocket.last = null
  const result = handler('remote:call')({}, computer, method, {}, 5_000)
  await settle()
  const socket = FakeSocket.last!
  socket.receive({ t: 'presence', online: true })
  return { socket, result }
}

test('a call reaches the computer through the relay, encrypted end to end', async () => {
  const { socket, result } = await connect('hello')
  assert.equal(socket.url, `wss://cloud.test/v1/relay/device/${computer.id}?v=1`)
  assert.equal(socket.headers['Codync-Sig'], `sig for /v1/relay/device/${computer.id}`)
  assert.equal(socket.sent[0]!.dk, device.publicKey)

  const h = host(socket)
  await settle()
  const [request] = h.inbox()
  assert.deepEqual({ m: request.m, b: request.b }, { m: 'hello', b: {} })
  h.reply({ id: request.id, ok: { name: 'Studio' } })
  // Built inside the module's own realm: compared by value.
  assert.equal(JSON.stringify(await result), '{"ok":true,"value":{"name":"Studio"}}')

  // The next call reuses the channel, and a subscription's events arrive until it ends.
  const sender = { sent: [] as unknown[][], isDestroyed: () => false, send: (...args: unknown[]) => sender.sent.push(args) }
  handler('remote:stream:open')({ sender }, 's1', computer, 'events', { since: 3, client: 'desktop' })
  await settle()
  assert.equal(FakeSocket.last, socket)
  const [sub] = h.inbox()
  assert.deepEqual({ sub: sub.sub, b: sub.b }, { sub: 'events', b: { since: 3, client: 'desktop' } })
  h.reply({ id: sub.id, ev: { type: 'resync' } })
  h.reply({ id: sub.id, end: true })
  assert.deepEqual(sender.sent, [['host:stream:data', 's1', '{"type":"resync"}'], ['host:stream:end', 's1', null]])
  closeAccountChannels()
})

test('a computer whose key changed is refused before any request goes out', async () => {
  const { socket, result } = await connect('hello')
  host(socket, crypto.randomBytes(32))
  const answer = await result
  assert.ok(!answer.ok)
  assert.match(answer.error.message, /identity changed/)
  assert.equal(socket.sent.filter((m) => m.t === 'f').length, 0)
})

test("a computer that doesn't let this device in ends its calls with 401, so nothing retries; a lapsed lease stays retryable", async () => {
  const refused = await connect('hello')
  refused.socket.receive({ t: 'reject', code: 'unauthorized' })
  const answer = await refused.result
  assert.ok(!answer.ok)
  assert.equal(answer.error.status, 401)

  const lapsed = await connect('hello')
  lapsed.socket.receive({ t: 'reject', code: 'leaseExpired' })
  const retry = await lapsed.result
  assert.ok(!retry.ok)
  assert.equal(retry.error.status, 0)
  closeAccountChannels()
})

test('a computer that restarts mid-link fails its calls; the next one opens a fresh channel', async () => {
  const { socket, result } = await connect('hello')
  host(socket)
  socket.receive({ t: 'presence', online: true })
  const answer = await result
  assert.ok(!answer.ok)
  assert.match(answer.error.message, /restarted/)
  const next = await connect('hello')
  assert.notEqual(next.socket, socket)
  closeAccountChannels()
  await next.result
})

test('concurrent first calls share one socket, which closes when its computer is let go', async () => {
  FakeSocket.last = null
  const first = handler('remote:call')({}, computer, 'hello', {}, 5_000)
  const second = handler('remote:call')({}, computer, 'bots', {}, 5_000)
  await settle()
  const socket = FakeSocket.last!
  socket.receive({ t: 'presence', online: true })
  const h = host(socket)
  await settle()
  assert.deepEqual(h.inbox().map((r) => r.m), ['hello', 'bots'])

  handler('remote:close')({}, computer.id)
  await settle()
  assert.equal(socket.readyState, 3)
  assert.deepEqual((await Promise.all([first, second])).map((a) => a.ok), [false, false])
})
