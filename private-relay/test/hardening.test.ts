import assert from 'node:assert/strict'
import { join } from 'node:path'
import { DatabaseSync } from 'node:sqlite'
import { PassThrough } from 'node:stream'
import test from 'node:test'
import type { WebSocket } from 'ws'
import { b64url } from '../src/codec.ts'
import type { RelayServer } from '../src/http.ts'
import { PING } from '../src/room.ts'
import {
  TOKEN,
  authorityOf,
  deviceSocket,
  hostSocket,
  nextClose,
  nextMessage,
  openSocket,
  refusedWith,
  register,
  registerAs,
  relay,
  signedAcl,
  vectors,
} from './harness.ts'
import { freshKey, sigHeader } from './sign.ts'

function nonceRows(dir: string): number {
  const db = new DatabaseSync(join(dir, 'relay.sqlite'))
  try {
    return (db.prepare('SELECT COUNT(*) AS n FROM nonces').get() as { n: number }).n
  } finally {
    db.close()
  }
}

test('a connection reset during the signature check closes only that socket', async () => {
  const { server } = await relay()
  try {
    const socket = new PassThrough()
    const req = {
      url: '/v1/relay/host?v=1',
      method: 'GET',
      headers: { host: authorityOf(server.url), 'codync-sig': 'v=1,kid=a,ts=1,nonce=b,sig=c' },
    }
    // The upgrade is now awaiting the signature check; a reset there used to throw out of the event loop.
    server.server.emit('upgrade', req, socket, Buffer.alloc(0))
    assert.doesNotThrow(() => socket.emit('error', Object.assign(new Error('read ECONNRESET'), { code: 'ECONNRESET' })))
    assert.equal((await fetch(`${server.url}/health`)).status, 200)
  } finally {
    await server.close()
  }
})

test('an unknown key is refused without storing its nonces, and an allowed request cannot be replayed', async () => {
  const { server, dir } = await relay()
  try {
    const outsider = freshKey()
    const authority = authorityOf(server.url)
    for (let i = 0; i < 3; i++) {
      const state = '/v1/host/state'
      const header = await sigHeader(outsider.seed, outsider.pub, 'GET', authority, state, new Uint8Array())
      assert.equal((await fetch(`${server.url}${state}`, { headers: { 'Codync-Sig': header } })).status, 404)
      assert.equal((await registerAs(server, outsider.seed, outsider.pub)).status, 403)
      const host = '/v1/relay/host?v=1'
      const hostHeader = await sigHeader(outsider.seed, outsider.pub, 'GET', authority, host, new Uint8Array())
      await assert.rejects(openSocket(`${server.url.replace('http', 'ws')}${host}`, hostHeader), refusedWith(404))
    }
    assert.equal(nonceRows(dir), 0)

    await register(server)
    const state = '/v1/host/state'
    const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'GET', authority, state, new Uint8Array())
    assert.equal((await fetch(`${server.url}${state}`, { headers: { 'Codync-Sig': header } })).status, 200)
    assert.equal((await fetch(`${server.url}${state}`, { headers: { 'Codync-Sig': header } })).status, 401)
  } finally {
    await server.close()
  }
})

/** A registered computer whose host published an ACL with the fixture device and `offers`, then went ready. */
async function readyHost(server: RelayServer, offers: { id: string; exp: number }[] = []): Promise<WebSocket> {
  await register(server)
  const host = await hostSocket(server)
  host.send(signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], offers))
  assert.equal(JSON.parse(await nextMessage(host)).t, 'acl.ok')
  host.send(JSON.stringify({ t: 'ready' }))
  return host
}

test('a device outside the ACL is refused, and so is every device before the first ACL', async () => {
  const { server } = await relay()
  try {
    await register(server)
    await assert.rejects(deviceSocket(server), refusedWith(404))
    const host = await hostSocket(server)
    host.send(signedAcl(1, [], []))
    assert.equal(JSON.parse(await nextMessage(host)).t, 'acl.ok')
    await assert.rejects(deviceSocket(server), refusedWith(403))
    host.close()
  } finally {
    await server.close()
  }
})

test('a new ACL closes a removed device and a pairing whose offer is gone', async () => {
  const { server } = await relay()
  try {
    const host = await readyHost(server, [{ id: vectors.pairing.offerId, exp: Date.now() + 60_000 }])
    const device = await deviceSocket(server)
    const pairing = await deviceSocket(server, vectors.pairing.offerId)
    const revoked = nextClose(device)
    const expired = nextClose(pairing)
    host.send(signedAcl(2, [], []))
    assert.equal(await revoked, 4003)
    assert.equal(await expired, 4410)
    host.close()
  } finally {
    await server.close()
  }
})

test('pairing sockets are capped, short-lived, limited in messages and kept off the mailbox', async () => {
  let now = Date.now()
  const { server } = await relay({ now: () => now })
  try {
    const host = await readyHost(server, [{ id: vectors.pairing.offerId, exp: now + 60_000 }])
    const first = await deviceSocket(server, vectors.pairing.offerId)
    const second = await deviceSocket(server, vectors.pairing.offerId)
    await assert.rejects(deviceSocket(server, vectors.pairing.offerId), refusedWith(429))
    await nextMessage(first)
    first.send(JSON.stringify({ t: 'mbox.list' }))
    assert.deepEqual(JSON.parse(await nextMessage(first)), { t: 'mbox.err', nonce: null, code: 'pairing' })
    const chatty = nextClose(first)
    for (let i = 0; i < 16; i++) first.send(PING)
    assert.equal(await chatty, 4008)
    const timedOut = nextClose(second)
    now += 60_000
    server.rooms.sweep()
    assert.equal(await timedOut, 4410)
    host.close()
  } finally {
    await server.close()
  }
})

test('a device sending more than 200 messages in 10 seconds is closed', async () => {
  const { server } = await relay()
  try {
    const host = await readyHost(server)
    const device = await deviceSocket(server)
    const closed = nextClose(device)
    for (let i = 0; i < 201; i++) device.send(PING)
    assert.equal(await closed, 4008)
    host.close()
  } finally {
    await server.close()
  }
})

test('a replayed WebSocket upgrade is refused', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const path = '/v1/relay/host?v=1'
    const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
    const url = `${server.url.replace('http', 'ws')}${path}`
    const host = await openSocket(url, header)
    await assert.rejects(openSocket(url, header), refusedWith(401))
    host.close()
  } finally {
    await server.close()
  }
})

test('an ACL for another computer is refused, even with this host signature', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const host = await hostSocket(server)
    host.send(signedAcl(1, [], [], 'A'.repeat(22)))
    assert.deepEqual(JSON.parse(await nextMessage(host)), { t: 'error', code: 'badAcl', ver: 0 })
    host.close()
  } finally {
    await server.close()
  }
})

test('the mailbox refuses an oversized blob and a device past its count', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const host = await hostSocket(server)
    host.send(signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], []))
    assert.equal(JSON.parse(await nextMessage(host)).t, 'acl.ok')
    const device = await deviceSocket(server)
    await nextMessage(device)
    const put = async (nonce: string, d = vectors.mailbox.blob) => {
      device.send(JSON.stringify({ t: 'mbox.put', nonce, d }))
      return JSON.parse(await nextMessage(device)) as { state?: string; code?: string }
    }
    assert.equal((await put('big', b64url(new Uint8Array(64 * 1024 + 1)))).code, 'tooLarge')
    for (let i = 0; i < 50; i++) assert.equal((await put(`n${i}`)).state, 'queued')
    assert.equal((await put('one-more')).code, 'full')
    host.close()
    device.close()
  } finally {
    await server.close()
  }
})

test('a signature made for another relay address does not verify here', async () => {
  const { server } = await relay({ publicUrl: 'https://Relay.Example.com:443' })
  try {
    const key = freshKey()
    assert.equal((await registerAs(server, key.seed, key.pub, TOKEN)).status, 401, 'signed for the address it was reached on')
    assert.equal((await registerAs(server, key.seed, key.pub, TOKEN, 'relay.example.com')).status, 200)
  } finally {
    await server.close()
  }
})

test('a connector sign-in returns to the app with its query', async () => {
  const { server } = await relay()
  try {
    for (const [path, scheme] of [['/v1/oauth/callback', 'codync'], ['/v1/oauth/callback/dev', 'codync-dev']]) {
      const res = await fetch(`${server.url}${path}?code=c&state=s`, { redirect: 'manual' })
      assert.equal(res.status, 302)
      assert.equal(res.headers.get('location'), `${scheme}://oauth?code=c&state=s`)
      assert.equal(res.headers.get('cache-control'), 'no-store')
    }
  } finally {
    await server.close()
  }
})
