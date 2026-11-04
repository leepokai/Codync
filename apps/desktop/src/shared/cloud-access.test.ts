import assert from 'node:assert/strict'
import * as crypto from 'node:crypto'
import path from 'node:path'
import { test } from 'node:test'
import * as channelCrypto from '../main/channel-crypto.ts'
import { loadMainModule, settle } from './test-support/main-module.ts'

type Handler = (event: unknown, ...args: unknown[]) => unknown
const handlers = new Map<string, Handler>()
const ipcMain = { handle: (channel: string, fn: Handler) => handlers.set(channel, fn), on: (channel: string, fn: Handler) => handlers.set(channel, fn) }

// Every session token is new, the way Clerk hands out short-lived ones.
let issued = 0
const account = { state: { cloudURL: 'https://cloud.test', user: { userId: 'user' } }, sessionToken: async () => `jwt${++issued}`, on() {} }

/** `true`: each 2-second wait ends at once; `false`: it lasts until the request is withdrawn. */
let quick = true
const sleep = (_ms: number, _value: unknown, { signal }: { signal: AbortSignal }) => new Promise((resolve, reject) => {
  if (quick) return resolve(undefined)
  signal.addEventListener('abort', () => reject(signal.reason), { once: true })
})

const hostNonce = crypto.randomBytes(32).toString('base64url')
let answered = false
const calls: string[] = []
const fetch = async (input: URL, init: RequestInit) => {
  calls.push(`${init.method} ${input.pathname} ${(init.headers as Record<string, string>).Authorization}`)
  const body = input.pathname.endsWith('/access-requests') ? { requestId: 'r1', expiresAt: Date.now() + 600_000 }
    : init.method === 'GET' ? { status: 'pending', ...(answered ? { hostNonce } : {}) }
    : {}
  // The computer answers after the first poll.
  if (init.method === 'GET') answered = true
  return new Response(JSON.stringify(body))
}

const { registerCloud } = loadMainModule<typeof import('../main/cloud.ts')>('cloud', {
  'node:crypto': crypto, 'node:path': path, 'node:timers/promises': { setTimeout: sleep },
  'node:fs': { promises: { readFile: async () => Promise.reject(new Error('ENOENT')), writeFile: async () => {}, mkdir: async () => {} } },
  electron: { app: { getPath: () => '/data' }, ipcMain, safeStorage: { isEncryptionAvailable: () => false } },
  './channel-crypto': channelCrypto, './account-channel': { registerAccountChannels() {}, closeAccountChannels() {} },
}, { URL, Buffer, AbortController, fetch })
registerCloud(account as never)

const signKey = (crypto.generateKeyPairSync('ed25519').publicKey.export({ format: 'jwk' }) as { x: string }).x
const computer = { id: channelCrypto.computerId(signKey), signKey }
const requestAccess = () => handlers.get('cloud:requestAccess')!({}, computer) as Promise<{ ok: boolean; value?: { code: string } }>

test('a slow access request asks for a current session token on every call', async () => {
  calls.length = 0
  quick = true
  answered = false
  const result = await requestAccess()
  assert.equal(result.ok, true)
  assert.match(result.value!.code, /^\d{6}$/)
  const tokens = calls.map((call) => call.split(' ').at(-1))
  assert.deepEqual(calls.map((call) => call.split(' ').slice(0, 2).join(' ')), [
    `POST /v1/computers/${computer.id}/access-requests`, 'GET /v1/access-requests/r1', 'GET /v1/access-requests/r1', 'POST /v1/access-requests/r1/reveal',
  ])
  assert.equal(new Set(tokens).size, tokens.length, 'no token is used twice')
})

test('cancelling before the computer shows its code withdraws the request', async () => {
  calls.length = 0
  quick = false
  answered = false
  const asking = requestAccess()
  for (let i = 0; i < 100 && !calls.some((call) => call.startsWith('GET')); i++) await settle()
  handlers.get('cloud:cancelAccess')!({}, computer.id)
  assert.equal((await asking).ok, false)
  assert.deepEqual(calls.map((call) => call.split(' ').slice(0, 2).join(' ')), [
    `POST /v1/computers/${computer.id}/access-requests`, 'GET /v1/access-requests/r1', 'DELETE /v1/access-requests/r1',
  ])
})
