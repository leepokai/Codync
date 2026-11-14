import assert from 'node:assert/strict'
import { copyFileSync, mkdtempSync } from 'node:fs'
import { request as httpRequest } from 'node:http'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { b64url, fromB64url } from '../src/codec.ts'
import { startRelay } from '../src/http.ts'
import { PONG } from '../src/room.ts'
import { RelayStore } from '../src/store.ts'
import {
  TOKEN,
  vectors,
  relay,
  authorityOf,
  register,
  openSocket,
  nextMessage,
  nextClose,
  hostSocket,
  deviceSocket,
  signedAcl,
  registerAs,
} from './harness.ts'
import { freshKey, sigHeader } from './sign.ts'

test('register, an empty state pull, and the cloud routes this relay does not serve', async () => {
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
    for (const route of ['/v1/host/screen-ice', '/v1/host/unclaim', '/v1/host/grants/g/revoke', `/v1/hooks/${vectors.keys.computerId}/h`]) {
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
    // The size refusal itself, not a later field check.
    assert.deepEqual(await declared.json(), { error: { code: 'badRequest', message: 'Body too large' } })
    const chunked = await new Promise<{ status: number; body: string }>((resolve, reject) => {
      const url = new URL(`${server.url}/v1/host/register`)
      const req = httpRequest(
        { hostname: url.hostname, port: url.port, path: url.pathname, method: 'POST', headers: { 'content-type': 'application/json' } },
        (res) => {
          let body = ''
          res.on('data', (chunk: Buffer) => (body += chunk.toString()))
          res.on('end', () => resolve({ status: res.statusCode ?? 0, body }))
        },
      )
      req.on('error', reject)
      req.write(over)
      req.end()
    })
    assert.equal(chunked.status, 400)
    assert.match(chunked.body, /Body too large/)
  } finally {
    await server.close()
  }
})

test('a restart between delivery and acknowledgment sends the item again', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'codync-private-relay-'))
  const file = join(dir, 'relay.sqlite')
  const store = new RelayStore(file)
  const server = await startRelay({ store, port: 0, token: TOKEN })
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
    assert.equal(store.mailboxByNonce(vectors.keys.computerId, vectors.keys.deviceSignPub, nonce)?.state, 'delivering')
    store.db.exec('PRAGMA wal_checkpoint(TRUNCATE)')
    copyFileSync(file, join(dir, 'crash.sqlite'))
    host.close()
    device.close()
  } finally {
    await server.close()
  }

  const recovered = new RelayStore(join(dir, 'crash.sqlite'))
  const again = await startRelay({ store: recovered, port: 0, token: TOKEN })
  try {
    assert.equal(recovered.mailboxByNonce(vectors.keys.computerId, vectors.keys.deviceSignPub, nonce)?.state, 'queued')
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
    assert.equal(server.rooms.retained(), 1)
    second.send(signedAcl(1, [{ dk: vectors.keys.deviceSignPub, grant: null }], []))
    assert.equal(JSON.parse(await nextMessage(second)).t, 'acl.ok')
    second.send(JSON.stringify({ t: 'ready' }))
    const device = await deviceSocket(server)
    assert.equal(JSON.parse(await nextMessage(device)).online, true)
    assert.equal((JSON.parse(await nextMessage(second)) as { t: string }).t, 'open')
    device.close()
    assert.equal((JSON.parse(await nextMessage(second)) as { t: string }).t, 'close')
    second.send(signedAcl(2, [], [{ id: vectors.pairing.offerId, exp: Date.now() - 1000 }]))
    assert.equal(JSON.parse(await nextMessage(second)).t, 'acl.ok')
    const path = `/v1/relay/device/${vectors.keys.computerId}?v=1&pair=${vectors.pairing.offerId}`
    const header = await sigHeader(vectors.keys.deviceSignSeed, vectors.keys.deviceSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
    await assert.rejects(openSocket(`${server.url.replace('http', 'ws')}${path}`, header), (error: { status?: number }) => error.status === 403)
    second.close()
  } finally {
    await server.close()
  }
})

test('a new computer needs the registration token, and a known one can refresh without it', async () => {
  const { server } = await relay()
  try {
    const outsider = freshKey()
    const refused = await registerAs(server, outsider.seed, outsider.pub)
    assert.equal(refused.status, 403)
    assert.equal(server.store.countComputers(), 0)
    const wrong = await registerAs(server, outsider.seed, outsider.pub, 'not-the-token')
    assert.equal(wrong.status, 403)
    assert.equal(server.store.countComputers(), 0)

    await register(server)
    const path = '/v1/host/register'
    const body = JSON.stringify({ boxKey: vectors.keys.hostBoxPub, name: 'Studio', platform: 'macos', version: '2.24.0' })
    const bytes = new TextEncoder().encode(body)
    const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'POST', authorityOf(server.url), path, bytes)
    const refresh = await fetch(`${server.url}${path}`, { method: 'POST', headers: { 'content-type': 'application/json', 'Codync-Sig': header }, body })
    assert.equal(refresh.status, 200)
    assert.equal(server.store.countComputers(), 1)
  } finally {
    await server.close()
  }
})

test('new enrollments stop at the cap and at the per-minute limit', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'codync-private-relay-'))
  const store = new RelayStore(join(dir, 'relay.sqlite'))
  const server = await startRelay({ store, port: 0, token: TOKEN, maxComputers: 1, newRegisterLimit: 2 })
  try {
    await register(server)
    const extra = freshKey()
    const full = await registerAs(server, extra.seed, extra.pub, TOKEN)
    assert.equal(full.status, 403)
    assert.equal(server.store.countComputers(), 1)

    const limited = await startRelay({ store: new RelayStore(join(dir, 'limit.sqlite')), port: 0, token: TOKEN, newRegisterLimit: 1 })
    try {
      const first = freshKey()
      const second = freshKey()
      assert.equal((await registerAs(limited, first.seed, first.pub, 'nope')).status, 403)
      const enrolled = await registerAs(limited, second.seed, second.pub, TOKEN)
      assert.equal(enrolled.status, 200)
      assert.equal(limited.store.countComputers(), 1)

      const blocked = await startRelay({
        store: new RelayStore(join(dir, 'reject.sqlite')),
        port: 0,
        token: TOKEN,
        rejectedRegisterLimit: 1,
      })
      try {
        const wrong = freshKey()
        const right = freshKey()
        const again = freshKey()
        assert.equal((await registerAs(blocked, wrong.seed, wrong.pub, 'nope')).status, 403)
        assert.equal((await registerAs(blocked, again.seed, again.pub, 'nope')).status, 429)
        assert.equal((await registerAs(blocked, right.seed, right.pub, TOKEN)).status, 200)
      } finally {
        await blocked.close()
      }
    } finally {
      await limited.close()
    }
  } finally {
    await server.close()
  }
})

test('a host that disconnects does not leave its room behind', async () => {
  const { server } = await relay()
  try {
    await register(server)
    const host = await hostSocket(server)
    assert.equal(server.rooms.retained(), 1)
    const closed = nextClose(host)
    host.close()
    await closed
    for (let i = 0; i < 20 && server.rooms.retained() !== 0; i++) {
      await new Promise((resolve) => setTimeout(resolve, 10))
    }
    assert.equal(server.rooms.retained(), 0)
  } finally {
    await server.close()
  }
})

test('expired mailbox rows are deleted when no room is open', async () => {
  let now = 1_000_000
  const dir = mkdtempSync(join(tmpdir(), 'codync-private-relay-'))
  const store = new RelayStore(join(dir, 'relay.sqlite'))
  const server = await startRelay({ store, port: 0, token: TOKEN, now: () => now })
  try {
    store.insertMailbox(vectors.keys.computerId, vectors.keys.deviceSignPub, 'gone', new Uint8Array(128), now + 1_000)
    store.insertMailbox(vectors.keys.computerId, vectors.keys.deviceSignPub, 'kept', new Uint8Array(128), now + 10_000)
    assert.equal(server.rooms.retained(), 0)
    now += 1_000
    server.rooms.sweep()
    assert.equal(store.mailboxByNonce(vectors.keys.computerId, vectors.keys.deviceSignPub, 'gone'), null)
    assert.equal(store.mailboxByNonce(vectors.keys.computerId, vectors.keys.deviceSignPub, 'kept')?.state, 'queued')
  } finally {
    await server.close()
  }
})
