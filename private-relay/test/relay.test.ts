import assert from 'node:assert/strict'
import { copyFileSync, mkdtempSync, readFileSync } from 'node:fs'
import { request as httpRequest } from 'node:http'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { WebSocket } from 'ws'
import { b64url, fromB64url } from '../src/codec.ts'
import { startRelay, type RelayServer } from '../src/http.ts'
import { PONG } from '../src/room.ts'
import { RelayStore } from '../src/store.ts'
import { sigHeader, signBytes } from './sign.ts'

const vectors = JSON.parse(readFileSync(new URL('../../docs/reference/fixtures/remote-relay-vectors.json', import.meta.url), 'utf8')) as {
  keys: { hostSignSeed: string; hostSignPub: string; hostBoxPub: string; deviceSignSeed: string; deviceSignPub: string; computerId: string }
  acl: { d: string; sig: string }
  mailbox: { blob: string; clientNonce: string }
  pairing: { offerId: string }
}

async function relay(): Promise<{ server: RelayServer; dir: string }> {
  const dir = mkdtempSync(join(tmpdir(), 'codync-private-relay-'))
  const store = new RelayStore(join(dir, 'relay.sqlite'))
  const server = await startRelay({ store, port: 0 })
  return { server, dir }
}

function authorityOf(url: string): string {
  return new URL(url).host
}

async function register(server: RelayServer): Promise<void> {
  const body = JSON.stringify({
    boxKey: vectors.keys.hostBoxPub,
    name: 'Studio',
    platform: 'macos',
    device: 'macstudio',
    version: '2.24.0',
  })
  const bytes = new TextEncoder().encode(body)
  const path = '/v1/host/register'
  const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'POST', authorityOf(server.url), path, bytes)
  const res = await fetch(`${server.url}${path}`, { method: 'POST', headers: { 'content-type': 'application/json', 'Codync-Sig': header }, body })
  assert.equal(res.status, 200)
  const json = (await res.json()) as { computerId: string; owned: boolean }
  assert.equal(json.computerId, vectors.keys.computerId)
  assert.equal(json.owned, false)
}

/** Messages that arrive before the test asks for them stay queued. */
function openSocket(url: string, header: string): Promise<WebSocket> {
  const ws = new WebSocket(url, { headers: { 'Codync-Sig': header } })
  const queued: string[] = []
  const waiters: ((text: string) => void)[] = []
  ws.on('message', (data) => {
    const text = data.toString()
    const waiter = waiters.shift()
    if (waiter) waiter(text)
    else queued.push(text)
  })
  let socketError: Error | null = null
  ws.once('error', (error) => {
    socketError = error instanceof Error ? error : new Error(String(error))
  })
  ;(ws as WebSocket & { take(): Promise<string> }).take = () =>
    new Promise((resolve, reject) => {
      if (socketError) return reject(socketError)
      const text = queued.shift()
      if (text !== undefined) resolve(text)
      else waiters.push(resolve)
    })
  return new Promise((resolve, reject) => {
    let settled = false
    const fail = (error: unknown) => {
      if (settled) return
      settled = true
      reject(error)
    }
    ws.once('open', () => {
      if (settled) return
      settled = true
      resolve(ws)
    })
    ws.once('unexpected-response', (_req, res) => {
      const chunks: Buffer[] = []
      res.on('data', (c: Buffer) => chunks.push(c))
      res.on('end', () => fail(Object.assign(new Error(`HTTP ${res.statusCode} ${Buffer.concat(chunks).toString()}`), { status: res.statusCode, body: Buffer.concat(chunks).toString() })))
    })
    ws.once('error', fail)
  })
}

function nextMessage(ws: WebSocket): Promise<string> {
  return (ws as WebSocket & { take(): Promise<string> }).take()
}

function nextClose(ws: WebSocket): Promise<number> {
  return new Promise((resolve) => ws.once('close', (code) => resolve(code)))
}

async function hostSocket(server: RelayServer): Promise<WebSocket> {
  const path = '/v1/relay/host?v=1'
  const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
  return openSocket(`${server.url.replace('http', 'ws')}${path}`, header)
}

async function deviceSocket(server: RelayServer, pair = ''): Promise<WebSocket> {
  const path = `/v1/relay/device/${vectors.keys.computerId}?v=1${pair ? `&pair=${pair}` : ''}`
  const header = await sigHeader(vectors.keys.deviceSignSeed, vectors.keys.deviceSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
  return openSocket(`${server.url.replace('http', 'ws')}${path}`, header)
}

function signedAcl(ver: number, devices: { dk: string; grant: null }[], offers: { id: string; exp: number }[]): string {
  const json = JSON.stringify({ v: 1, computerId: vectors.keys.computerId, ver, devices, offers })
  const d = b64url(new TextEncoder().encode(json))
  const sig = signBytes(vectors.keys.hostSignSeed, new TextEncoder().encode(json))
  return JSON.stringify({ t: 'acl', d, sig })
}

test('register, an empty state pull, and phase-1 routes that stay 404', async () => {
  const { server } = await relay()
  try {
    assert.equal((await fetch(`${server.url}/health`)).status, 200)
    await register(server)
    const path = '/v1/host/state'
    const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
    const state = await fetch(`${server.url}${path}`, { headers: { 'Codync-Sig': header } })
    assert.deepEqual(await state.json(), { owner: null, grants: [], requests: [] })
    const old = '/v1/relay/host?v=2'
    const oldHeader = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'GET', authorityOf(server.url), old, new Uint8Array())
    await assert.rejects(openSocket(`${server.url.replace('http', 'ws')}${old}`, oldHeader), (error: { status?: number }) => error.status === 426)
    for (const route of ['/v1/host/screen-ice', '/v1/host/unclaim', '/v1/host/grants/g/revoke']) {
      const res = await fetch(`${server.url}${route}`, { method: 'POST', body: '{}' })
      assert.equal(res.status, 404, route)
      assert.equal(((await res.json()) as { error: { code: string } }).error.code, 'notFound')
    }
  } finally {
    await server.close()
  }
})

test('a pair that is not 16 bytes is refused and creates no computer', async () => {
  const { server } = await relay()
  try {
    const path = `/v1/relay/device/${vectors.keys.computerId}?v=1&pair=abc`
    const header = await sigHeader(vectors.keys.deviceSignSeed, vectors.keys.deviceSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
    await assert.rejects(openSocket(`${server.url.replace('http', 'ws')}${path}`, header), (error: { status?: number }) => error.status === 400)
    assert.equal(server.store.countComputers(), 0)
  } finally {
    await server.close()
  }
})

test('the host publishes an ACL, devices are capped, and pong bytes are exact', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const host = await hostSocket(server)
    const acl = signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], [{ id: vectors.pairing.offerId, exp: Date.now() + 60_000 }])
    const ok = nextMessage(host)
    host.send(acl)
    assert.deepEqual(JSON.parse(await ok), { t: 'acl.ok', ver: 1 })

    const stale = nextMessage(host)
    host.send(acl)
    const staleMsg = JSON.parse(await stale) as { t: string; code: string; ver: number }
    assert.deepEqual(staleMsg, { t: 'error', code: 'staleVersion', ver: 1 })

    const bad = nextMessage(host)
    host.send(JSON.stringify({ t: 'acl', d: b64url(new TextEncoder().encode('{}')), sig: vectors.acl.sig }))
    const badMsg = JSON.parse(await bad) as { code: string; ver: number }
    assert.equal(badMsg.code, 'badAcl')
    assert.equal(badMsg.ver, 1)

    host.send(JSON.stringify({ t: 'ready' }))
    const device = await deviceSocket(server)
    const presence = JSON.parse(await nextMessage(device)) as { t: string; online: boolean }
    assert.equal(presence.t, 'presence')
    assert.equal(presence.online, true)
    const pong = nextMessage(device)
    device.send('{"t":"ping"}')
    assert.equal(await pong, PONG)
    const spaced = nextMessage(device)
    device.send('{"t":"ping","x":1}')
    assert.equal(await spaced, PONG)

    const sockets = [device]
    for (let i = 1; i < 16; i++) sockets.push(await deviceSocket(server))
    await assert.rejects(deviceSocket(server), (error: { status?: number }) => error.status === 429)
    for (const socket of sockets) socket.close()
    host.close()
  } finally {
    await server.close()
  }
})

test('mailbox put is idempotent, refused while the host is ready, and delivered one at a time', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const host = await hostSocket(server)
    host.send(signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], []))
    assert.equal(JSON.parse(await nextMessage(host)).t, 'acl.ok')
    const device = await deviceSocket(server)
    await nextMessage(device)
    const blob = vectors.mailbox.blob
    assert.ok((fromB64url(blob)?.length ?? 0) >= 112)
    const put = (nonce: string) => JSON.stringify({ t: 'mbox.put', nonce, d: blob })
    device.send(put(vectors.mailbox.clientNonce))
    assert.deepEqual(JSON.parse(await nextMessage(device)), { t: 'mbox.ok', nonce: vectors.mailbox.clientNonce, state: 'queued' })
    device.send(put(vectors.mailbox.clientNonce))
    assert.deepEqual(JSON.parse(await nextMessage(device)), { t: 'mbox.ok', nonce: vectors.mailbox.clientNonce, state: 'queued' })
    device.send(JSON.stringify({ t: 'mbox.put', nonce: 'short', d: b64url(new Uint8Array(16)) }))
    assert.equal(JSON.parse(await nextMessage(device)).code, 'invalid')
    device.send(JSON.stringify({ t: 'mbox.list' }))
    const listed = JSON.parse(await nextMessage(device)) as { items: { nonce: string }[] }
    assert.equal(listed.items.length, 1)

    const second = 'second-nonce'
    device.send(put(second))
    assert.equal(JSON.parse(await nextMessage(device)).state, 'queued')
    host.send(JSON.stringify({ t: 'ready' }))
    const open = JSON.parse(await nextMessage(host)) as { t: string }
    const firstItem = JSON.parse(await nextMessage(host)) as { t: string; seq: number; nonce: string }
    assert.equal(open.t, 'open')
    assert.equal(firstItem.t, 'mbox.item')
    assert.equal((JSON.parse(await nextMessage(device)) as { online: boolean }).online, true)
    device.send(put('while-online'))
    assert.equal(JSON.parse(await nextMessage(device)).code, 'hostOnline')
    host.send(JSON.stringify({ t: 'mbox.ack', seq: firstItem.seq, ok: true }))
    assert.equal(JSON.parse(await nextMessage(device)).t, 'mbox.delivered')
    const next = JSON.parse(await nextMessage(host)) as { t: string; nonce: string; seq: number }
    assert.equal(next.t, 'mbox.item')
    assert.equal(next.nonce, second)
    host.send(JSON.stringify({ t: 'mbox.ack', seq: next.seq, ok: false, code: 'nope' }))
    const failed = JSON.parse(await nextMessage(device)) as { t: string; code: string }
    assert.equal(failed.t, 'mbox.failed')
    assert.equal(failed.code, 'invalid')
    device.send(JSON.stringify({ t: 'mbox.cancel', nonce: 'missing' }))
    assert.equal(JSON.parse(await nextMessage(device)).t, 'mbox.gone')
    host.close()
    device.close()
  } finally {
    await server.close()
  }
})

test('a null message closes only the socket that sent it', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const host = await hostSocket(server)
    host.send(signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], []))
    assert.equal(JSON.parse(await nextMessage(host)).t, 'acl.ok')
    const device = await deviceSocket(server)
    await nextMessage(device)
    const other = await deviceSocket(server)
    await nextMessage(other)

    const closed = nextClose(device)
    device.send('null')
    assert.equal(await closed, 4002)

    host.send('null')
    host.send('true')
    host.send('[]')
    const hostPong = nextMessage(host)
    host.send('{"t":"ping","n":1}')
    assert.equal(await hostPong, PONG)
    const devicePong = nextMessage(other)
    other.send('{"t":"ping","n":1}')
    assert.equal(await devicePong, PONG)
    assert.equal((await fetch(`${server.url}/health`)).status, 200)
    host.close()
    other.close()
  } finally {
    await server.close()
  }
})

test('a body over the limit is refused before it is trusted', async () => {
  const { server } = await relay()
  try {
    const over = Buffer.alloc(64 * 1024 + 1, 0x61)
    const declared = await fetch(`${server.url}/v1/host/register`, { method: 'POST', body: over })
    assert.equal(declared.status, 400)
    assert.equal(((await declared.json()) as { error: { code: string } }).error.code, 'badRequest')
    const chunked = await new Promise<number>((resolve, reject) => {
      const url = new URL(`${server.url}/v1/host/register`)
      const req = httpRequest(
        { hostname: url.hostname, port: url.port, path: url.pathname, method: 'POST', headers: { 'content-type': 'application/json' } },
        (res) => {
          res.resume()
          res.on('end', () => resolve(res.statusCode ?? 0))
        },
      )
      req.on('error', reject)
      req.write(over)
      req.end()
    })
    assert.equal(chunked, 400)
  } finally {
    await server.close()
  }
})

test('a restart between delivery and acknowledgment sends the item again', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'codync-private-relay-'))
  const file = join(dir, 'relay.sqlite')
  const store = new RelayStore(file)
  const server = await startRelay({ store, port: 0 })
  const nonce = vectors.mailbox.clientNonce
  try {
    await register(server)
    const host = await hostSocket(server)
    host.send(signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], []))
    assert.equal(JSON.parse(await nextMessage(host)).t, 'acl.ok')
    const device = await deviceSocket(server)
    await nextMessage(device)
    device.send(JSON.stringify({ t: 'mbox.put', nonce, d: vectors.mailbox.blob }))
    assert.equal(JSON.parse(await nextMessage(device)).state, 'queued')
    host.send(JSON.stringify({ t: 'ready' }))
    assert.equal((JSON.parse(await nextMessage(host)) as { t: string }).t, 'open')
    const item = JSON.parse(await nextMessage(host)) as { t: string; nonce: string }
    assert.equal(item.t, 'mbox.item')
    assert.equal(item.nonce, nonce)
    assert.equal(store.mailboxState(vectors.keys.computerId, vectors.keys.deviceSignPub, nonce), 'delivering')
    store.db.exec('PRAGMA wal_checkpoint(TRUNCATE)')
    copyFileSync(file, join(dir, 'crash.sqlite'))
    host.close()
    device.close()
  } finally {
    await server.close()
  }

  const recovered = new RelayStore(join(dir, 'crash.sqlite'))
  const again = await startRelay({ store: recovered, port: 0 })
  try {
    assert.equal(recovered.mailboxState(vectors.keys.computerId, vectors.keys.deviceSignPub, nonce), 'queued')
    const host = await hostSocket(again)
    host.send(JSON.stringify({ t: 'ready' }))
    const item = JSON.parse(await nextMessage(host)) as { t: string; nonce: string }
    assert.equal(item.t, 'mbox.item')
    assert.equal(item.nonce, nonce)
    host.close()
  } finally {
    await again.close()
  }
})

test('a second host replaces the first, and an expired offer is forbidden', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const first = await hostSocket(server)
    const replaced = nextClose(first)
    const second = await hostSocket(server)
    assert.equal(await replaced, 4009)
    second.send(signedAcl(1, [], [{ id: vectors.pairing.offerId, exp: Date.now() - 1000 }]))
    assert.equal(JSON.parse(await nextMessage(second)).t, 'acl.ok')
    const path = `/v1/relay/device/${vectors.keys.computerId}?v=1&pair=${vectors.pairing.offerId}`
    const header = await sigHeader(vectors.keys.deviceSignSeed, vectors.keys.deviceSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
    await assert.rejects(openSocket(`${server.url.replace('http', 'ws')}${path}`, header), (error: { status?: number }) => error.status === 403)
    second.close()
  } finally {
    await server.close()
  }
})
