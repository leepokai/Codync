/** HTTP registration and the WebSocket upgrade. The upgrade is accepted only after admission. */

import { timingSafeEqual } from 'node:crypto'
import { createServer, STATUS_CODES, type IncomingMessage, type Server } from 'node:http'
import type { Duplex } from 'node:stream'
import { Hono } from 'hono'
import { getRequestListener } from '@hono/node-server'
import { HTTPException } from 'hono/http-exception'
import { WebSocketServer } from 'ws'
import { COMPUTER_ID, computerIdFor, fromB64url } from './codec.ts'
import { publicAuthority, verifyCodyncSig } from './sig.ts'
import type { RelayStore } from './store.ts'
import { Rooms } from './room.ts'

const MAX_BODY = 64 * 1024
const MAX_COMPUTERS = 32
const REGISTER_WINDOW_MS = 60_000
const REGISTER_LIMIT = 10
const REJECT_LIMIT = 30
const PLATFORMS = ['macos', 'linux', 'windows'] as const

/** Equal-length compare. A length mismatch is a miss; the token is not a secret length. */
function sameToken(given: string, expected: string): boolean {
  const left = Buffer.from(given)
  const right = Buffer.from(expected)
  return left.length === right.length && timingSafeEqual(left, right)
}

/** At most `limit` attempts in any `REGISTER_WINDOW_MS`. */
class Window {
  private readonly attempts: number[] = []
  private readonly limit: number

  constructor(limit: number) {
    this.limit = limit
  }

  /** Counts one attempt at `at`, or `false` when the window is already full. */
  take(at: number): boolean {
    while (this.attempts.length && at - (this.attempts[0] ?? at) >= REGISTER_WINDOW_MS) this.attempts.shift()
    if (this.attempts.length >= this.limit) return false
    this.attempts.push(at)
    return true
  }
}

function jsonError(status: number, code: string, message?: string): Response {
  return Response.json({ error: { code, ...(message ? { message } : {}) } }, { status })
}

/**
 * Ends the request with `jsonError`. Thrown rather than returned: `@hono/node-server` swaps the
 * global `Response`, so `instanceof Response` is false for what `Response.json` makes.
 */
function refuse(status: 400 | 401 | 403 | 404 | 429, code: string, message?: string): never {
  throw new HTTPException(status, { res: jsonError(status, code, message) })
}

function reject(socket: Duplex, status: number, code: string): void {
  const body = JSON.stringify({ error: { code } })
  socket.write(
    `HTTP/1.1 ${status} ${STATUS_CODES[status] ?? 'Error'}\r\nContent-Type: application/json\r\nContent-Length: ${Buffer.byteLength(body)}\r\nConnection: close\r\n\r\n${body}`,
  )
  socket.destroy()
}

function objectBody(raw: Uint8Array): Record<string, unknown> {
  if (!raw.length) return {}
  let value: unknown
  try {
    value = JSON.parse(new TextDecoder().decode(raw))
  } catch {
    refuse(400, 'badRequest', 'Body is not JSON')
  }
  if (!value || typeof value !== 'object' || Array.isArray(value)) refuse(400, 'badRequest', 'Body must be an object')
  return value as Record<string, unknown>
}

interface Registration {
  boxPub: string
  name: string
  platform: string
  device: string | null
  version: string
}

/** The register body's fields, checked. */
function parseRegister(body: Record<string, unknown>): Registration {
  const { boxKey, name, version, platform, device } = body
  if (!fromB64url(boxKey, 32) || typeof name !== 'string' || !name.trim() || name.length > 100) refuse(400, 'badRequest', 'Invalid register')
  if (typeof version !== 'string' || !version.trim() || version.length > 32) refuse(400, 'badRequest', 'Invalid register')
  if (typeof platform !== 'string' || !PLATFORMS.includes(platform as (typeof PLATFORMS)[number])) refuse(400, 'badRequest', 'Invalid platform')
  if (device != null && (typeof device !== 'string' || device.length > 32)) refuse(400, 'badRequest', 'Invalid device')
  return { boxPub: boxKey as string, name, platform, device: typeof device === 'string' ? device : null, version }
}

/** Reads at most `MAX_BODY` bytes, including a request with no `Content-Length`. */
async function limitedBody(request: Request): Promise<Uint8Array> {
  const declared = request.headers.get('content-length')
  if (declared !== null && (!/^\d+$/.test(declared) || Number(declared) > MAX_BODY)) {
    await request.body?.cancel().catch(() => {
      // The client may already be gone. The size refusal still stands.
    })
    refuse(400, 'badRequest', 'Body too large')
  }
  const reader = request.body?.getReader()
  if (!reader) return new Uint8Array()
  const chunks: Uint8Array[] = []
  let total = 0
  try {
    for (;;) {
      const { done, value } = await reader.read()
      if (done) break
      if (!value?.byteLength) continue
      total += value.byteLength
      if (total > MAX_BODY) {
        await reader.cancel().catch(() => {
          // The client may already be gone. The size refusal still stands.
        })
        break
      }
      chunks.push(value)
    }
  } catch {
    refuse(400, 'badRequest', 'Body is not JSON')
  }
  if (total > MAX_BODY) refuse(400, 'badRequest', 'Body too large')
  return Buffer.concat(chunks)
}

/** A valid `Codync-Sig` and the computer its key derives. */
interface Signed {
  kid: string
  nonce: string
  computerId: string
}

export interface RelayServer {
  server: Server
  url: string
  store: RelayStore
  rooms: Rooms
  close: () => Promise<void>
}

export async function startRelay(options: {
  store: RelayStore
  host?: string
  port?: number
  now?: () => number
  /**
   * The host (and non-default port) computers put in their Relay address, which `Codync-Sig`
   * signs. Defaults to the listening address. Never taken from the request's `Host` header, so a
   * signature made for another relay does not verify here.
   */
  publicUrl?: string
  /** Required to enroll a computer this relay does not already know. */
  token?: string
  maxComputers?: number
  /** New enrollments allowed per minute. A known computer refreshing its row does not count. */
  newRegisterLimit?: number
  /** Wrong-token attempts allowed per minute. They do not consume `newRegisterLimit`. */
  rejectedRegisterLimit?: number
}): Promise<RelayServer> {
  const now = options.now ?? Date.now
  const rooms = new Rooms(options.store, now)
  const store = options.store
  const token = options.token ?? ''
  const maxComputers = options.maxComputers ?? MAX_COMPUTERS
  const enrollments = new Window(options.newRegisterLimit ?? REGISTER_LIMIT)
  const rejections = new Window(options.rejectedRegisterLimit ?? REJECT_LIMIT)
  // Set once the port is bound, before the first request can arrive.
  let authority = ''

  /** Any key can sign, so callers spend the nonce (`rememberNonce`) only once the request is
   * otherwise allowed: unknown keys never add rows. */
  async function verify(header: string | undefined, method: string, url: URL, body: Uint8Array): Promise<Signed | null> {
    const signed = await verifyCodyncSig(header, method, authority, url.pathname + url.search, body, now())
    if (!signed) return null
    // `verifyCodyncSig` already checked `kid` is a 32-byte key.
    const pub = fromB64url(signed.kid, 32) as Uint8Array
    return { kid: signed.kid, nonce: signed.nonce, computerId: await computerIdFor(pub) }
  }

  const app = new Hono()

  app.get('/health', (c) => c.json({ ok: true }))

  // A connector's sign-in on the phone returns here; the query goes on to the app (cloud/src/api.ts).
  // The code in it is useless without the PKCE verifier only the host holds.
  const bounce = (scheme: string) => (c: { req: { url: string } }) =>
    new Response(null, { status: 302, headers: { Location: `${scheme}://oauth${new URL(c.req.url).search}`, 'Cache-Control': 'no-store' } })
  app.get('/v1/oauth/callback', bounce('codync'))
  app.get('/v1/oauth/callback/dev', bounce('codync-dev'))

  app.post('/v1/host/register', async (c) => {
    const raw = await limitedBody(c.req.raw)
    const body = objectBody(raw)
    const signed = await verify(c.req.header('codync-sig'), 'POST', new URL(c.req.url), raw)
    if (!signed) return jsonError(401, 'badSignature')
    const fields = parseRegister(body)
    const existing = store.computer(signed.computerId)
    if (existing && existing.status !== 'active') return jsonError(403, 'forbidden')
    // A key this relay already accepted can refresh. Any other key needs the operator's token,
    // and the table stops growing once it is full.
    if (!existing) {
      const at = now()
      // A wrong token must not use up the allowance a real computer needs to enroll.
      if (!token || !sameToken(c.req.header('codync-relay-token') ?? '', token)) {
        if (!rejections.take(at)) return jsonError(429, 'rateLimited')
        return jsonError(403, 'forbidden', 'This relay needs its relay token to add a new computer.')
      }
      if (!enrollments.take(at)) return jsonError(429, 'rateLimited')
      if (store.countComputers() >= maxComputers) return jsonError(403, 'forbidden', 'This relay already has as many computers as it keeps.')
    }
    if (!store.rememberNonce(signed.kid, signed.nonce, now())) return jsonError(401, 'badSignature')
    store.register({ id: signed.computerId, signPub: signed.kid, ...fields, now: now() })
    return c.json({ computerId: signed.computerId, owned: false })
  })

  app.get('/v1/host/state', async (c) => {
    const signed = await verify(c.req.header('codync-sig'), 'GET', new URL(c.req.url), new Uint8Array())
    if (!signed) return jsonError(401, 'badSignature')
    const computer = store.computer(signed.computerId)
    if (!computer || computer.status !== 'active') return jsonError(404, 'unknownComputer')
    if (!store.rememberNonce(signed.kid, signed.nonce, now())) return jsonError(401, 'badSignature')
    return c.json({ owner: null, grants: [], requests: [] })
  })

  // Accounts, screen TURN and public webhooks are not served here.
  app.notFound(() => jsonError(404, 'notFound'))

  const listener = getRequestListener(app.fetch)
  const server = createServer((req, res) => {
    listener(req, res)
  })
  const wss = new WebSocketServer({ noServer: true, maxPayload: 1024 * 1024 })
  const sweep = setInterval(() => rooms.sweep(), 30_000)
  sweep.unref?.()

  /** Admits the socket to a room, or refuses it with an HTTP answer. */
  async function upgrade(req: IncomingMessage, socket: Duplex, head: Buffer): Promise<void> {
    const url = new URL(req.url ?? '/', 'http://relay.local')
    if (url.searchParams.get('v') !== '1') return reject(socket, 426, 'upgradeRequired')
    if (req.method !== 'GET') return reject(socket, 400, 'badRequest')
    const header = req.headers['codync-sig']
    const signed = Array.isArray(header) ? null : await verify(header, 'GET', url, new Uint8Array())
    if (!signed) return reject(socket, 401, 'badSignature')

    if (url.pathname === '/v1/relay/host') {
      const admitted = rooms.admitHost(signed.computerId, signed.kid)
      if (!admitted.ok) return reject(socket, admitted.status, admitted.code)
      if (!store.rememberNonce(signed.kid, signed.nonce, now())) return reject(socket, 401, 'badSignature')
      wss.handleUpgrade(req, socket, head, (ws) => rooms.attachHost(signed.computerId, signed.kid, ws))
      return
    }

    const computerId = url.pathname.match(/^\/v1\/relay\/device\/([^/]+)$/)?.[1]
    if (!computerId) return reject(socket, 404, 'notFound')
    if (!COMPUTER_ID.test(computerId)) return reject(socket, 400, 'badRequest')
    const pair = url.searchParams.get('pair') ?? ''
    if (pair && !fromB64url(pair, 16)) return reject(socket, 400, 'badRequest')
    const admitted = rooms.admitDevice(computerId, signed.kid, pair)
    if (!admitted.ok) return reject(socket, admitted.status, admitted.code)
    if (!store.rememberNonce(signed.kid, signed.nonce, now())) return reject(socket, 401, 'badSignature')
    wss.handleUpgrade(req, socket, head, (ws) => rooms.attachDevice(computerId, signed.kid, admitted, ws))
  }

  server.on('upgrade', (req, socket, head) => {
    // Node hands over the raw socket. Until `ws` takes it, an unhandled 'error' (a reset during
    // the signature check) would throw out of the event loop and stop the whole relay.
    socket.on('error', () => socket.destroy())
    void upgrade(req, socket, head).catch((error: unknown) => {
      console.error('upgrade failed', error)
      socket.destroy()
    })
  })

  const host = options.host ?? '127.0.0.1'
  await new Promise<void>((resolve) => server.listen(options.port ?? 0, host, resolve))
  const address = server.address()
  const bound = typeof address === 'object' && address ? address.port : (options.port ?? 0)
  authority = publicAuthority(options.publicUrl ?? `http://${host}:${bound}`)
  return {
    server,
    url: `http://${host}:${bound}`,
    store,
    rooms,
    close: () =>
      new Promise((resolve, reject) => {
        clearInterval(sweep)
        for (const client of wss.clients) client.terminate()
        wss.close()
        server.close((error) => (error ? reject(error) : resolve()))
      }),
  }
}
