/** Shared setup for the relay tests: a relay on a temp store, the fixture keys, and signed sockets. */

import assert from 'node:assert/strict'
import { mkdtempSync, readFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { WebSocket } from 'ws'
import { b64url } from '../src/codec.ts'
import { startRelay, type RelayServer } from '../src/http.ts'
import { RelayStore } from '../src/store.ts'
import { sigHeader, signBytes } from './sign.ts'

export const TOKEN = 'test-relay-token-1'

export const vectors = JSON.parse(readFileSync(new URL('../../docs/reference/fixtures/remote-relay-vectors.json', import.meta.url), 'utf8')) as {
  keys: { hostSignSeed: string; hostSignPub: string; hostBoxPub: string; deviceSignSeed: string; deviceSignPub: string; computerId: string }
  acl: { d: string; sig: string }
  mailbox: { blob: string; clientNonce: string }
  pairing: { offerId: string }
}

export async function relay(options: { now?: () => number; publicUrl?: string } = {}): Promise<{ server: RelayServer; dir: string }> {
  const dir = mkdtempSync(join(tmpdir(), 'codync-private-relay-'))
  const store = new RelayStore(join(dir, 'relay.sqlite'))
  const server = await startRelay({ store, port: 0, token: TOKEN, ...options })
  return { server, dir }
}

/** For `assert.rejects`: the upgrade was answered with this HTTP status. */
export const refusedWith = (status: number) => (error: { status?: number }) => error.status === status

export function authorityOf(url: string): string {
  return new URL(url).host
}

export async function register(server: RelayServer): Promise<void> {
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
  const res = await fetch(`${server.url}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json', 'Codync-Sig': header, 'Codync-Relay-Token': TOKEN },
    body,
  })
  assert.equal(res.status, 200)
  const json = (await res.json()) as { computerId: string; owned: boolean }
  assert.equal(json.computerId, vectors.keys.computerId)
  assert.equal(json.owned, false)
}

/** Messages that arrive before the test asks for them stay queued. */
export function openSocket(url: string, header: string): Promise<WebSocket> {
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

export function nextMessage(ws: WebSocket): Promise<string> {
  return (ws as WebSocket & { take(): Promise<string> }).take()
}

export function nextClose(ws: WebSocket): Promise<number> {
  return new Promise((resolve) => ws.once('close', (code) => resolve(code)))
}

export async function hostSocket(server: RelayServer): Promise<WebSocket> {
  const path = '/v1/relay/host?v=1'
  const header = await sigHeader(vectors.keys.hostSignSeed, vectors.keys.hostSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
  return openSocket(`${server.url.replace('http', 'ws')}${path}`, header)
}

export async function deviceSocket(server: RelayServer, pair = ''): Promise<WebSocket> {
  const path = `/v1/relay/device/${vectors.keys.computerId}?v=1${pair ? `&pair=${pair}` : ''}`
  const header = await sigHeader(vectors.keys.deviceSignSeed, vectors.keys.deviceSignPub, 'GET', authorityOf(server.url), path, new Uint8Array())
  return openSocket(`${server.url.replace('http', 'ws')}${path}`, header)
}

export function signedAcl(
  ver: number,
  devices: { dk: string; grant: null }[],
  offers: { id: string; exp: number }[],
  computerId = vectors.keys.computerId,
): string {
  const json = JSON.stringify({ v: 1, computerId, ver, devices, offers })
  const d = b64url(new TextEncoder().encode(json))
  const sig = signBytes(vectors.keys.hostSignSeed, new TextEncoder().encode(json))
  return JSON.stringify({ t: 'acl', d, sig })
}

export async function registerAs(server: RelayServer, seed: string, pub: string, token?: string, authority = authorityOf(server.url)): Promise<Response> {
  const body = JSON.stringify({ boxKey: vectors.keys.hostBoxPub, name: 'Other', platform: 'linux', version: '2.24.0' })
  const bytes = new TextEncoder().encode(body)
  const path = '/v1/host/register'
  const header = await sigHeader(seed, pub, 'POST', authority, path, bytes)
  const headers: Record<string, string> = { 'content-type': 'application/json', 'Codync-Sig': header }
  if (token !== undefined) headers['Codync-Relay-Token'] = token
  return fetch(`${server.url}${path}`, { method: 'POST', headers, body })
}
