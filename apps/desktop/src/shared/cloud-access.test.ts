import assert from 'node:assert/strict'
import * as crypto from 'node:crypto'
import { test } from 'node:test'
import * as channelCrypto from '../main/channel/crypto.ts'
import { deferred, loadMainModule, settle } from './test-support/main-module.ts'

type Handler = (event: unknown, ...args: unknown[]) => unknown
const handlers = new Map<string, Handler>()
const ipcMain = { handle: (channel: string, fn: Handler) => handlers.set(channel, fn), on: (channel: string, fn: Handler) => handlers.set(channel, fn) }

/** `true`: each poll wait ends at once; `false`: it lasts until the request is withdrawn. */
let quick = true
const sleep = (_ms: number, _value: unknown, { signal }: { signal: AbortSignal }) => new Promise((resolve, reject) => {
  if (quick) return resolve(undefined)
  signal.addEventListener('abort', () => reject(signal.reason), { once: true })
})

const deviceKey = crypto.generateKeyPairSync('ed25519').publicKey
const devicePub = (deviceKey.export({ format: 'jwk' }) as { x: string }).x
const hostNonce = crypto.randomBytes(32).toString('base64url')
/** The computer's answers, in order, to each status poll. */
let polls: Record<string, unknown>[] = []
const calls: string[] = []
let posts = 0
/** When set, the next POST waits for it before answering, as a slow network would. */
let holdPost: Promise<void> | null = null
const cloudRequest = async (_account: unknown, method: string, path: string, body: unknown) => {
  calls.push(`${method} ${path}`)
  if (path.endsWith('/access-requests')) {
    if (holdPost) await holdPost
    return { ok: true, value: { requestId: `r${++posts}`, expiresAt: Date.now() + 600_000, commit: body } }
  }
  if (method === 'GET') return { ok: true, value: polls.length > 1 ? polls.shift()! : polls[0]! }
  return { ok: true, value: {} }
}

const { registerCloudAccess } = loadMainModule<typeof import('../main/cloud-access.ts')>('cloud-access', {
  'node:crypto': crypto, 'node:timers/promises': { setTimeout: sleep }, electron: { ipcMain },
  './channel/crypto.ts': channelCrypto, './cloud': { cloudRequest, identity: async () => ({ publicKey: devicePub }) },
}, { AbortController, Buffer })
registerCloudAccess({ state: { user: { userId: 'user' } } } as never)

const signKey = (crypto.generateKeyPairSync('ed25519').publicKey.export({ format: 'jwk' }) as { x: string }).x
const computer = { computerId: 'studio', signKey }
const sent: unknown[][] = []
const sender = { isDestroyed: () => false, send: (...args: unknown[]) => void sent.push(args) }
type Result = { ok: boolean; value?: { status: string } }
/** How the request ended: its status, or the failure; read field by field, the module's objects are another realm's. */
const ending = async (result: Promise<Result>) => {
  const res = await result
  return res.ok ? res.value!.status : 'failed'
}
const requestAccess = () => handlers.get('cloud:requestAccess')!({ sender }, computer, 'ask-1') as Promise<Result>
const steps = () => calls.map((call) => call.replace('v1/', ''))

test('the code the computer shows goes to the page once ours is revealed, then its decision ends the request', async () => {
  calls.length = sent.length = 0
  posts = 0
  quick = true
  polls = [{ status: 'pending' }, { status: 'pending', hostNonce }, { status: 'approved' }]
  assert.equal(await ending(requestAccess()), 'approved')
  assert.deepEqual(steps(), ['POST computers/studio/access-requests', 'GET access-requests/r1', 'GET access-requests/r1', 'POST access-requests/r1/reveal', 'GET access-requests/r1'])
  assert.equal(sent.length, 1)
  const [channel, requestId, code] = sent[0]!
  assert.deepEqual([channel, requestId], ['cloud:accessCode', 'ask-1'])
  assert.match(String(code), /^\d{6}$/)
})

test('cancelling withdraws the request from the computer, before or after its code', async () => {
  for (const answered of [false, true]) {
    calls.length = sent.length = 0
    posts = 0
    quick = false
    polls = [answered ? { status: 'pending', hostNonce } : { status: 'pending' }]
    const asking = requestAccess()
    for (let i = 0; i < 100 && !calls.at(-1)?.startsWith('GET'); i++) await settle()
    handlers.get('cloud:cancelAccess')!({}, 'studio')
    assert.equal(await ending(asking), 'cancelled')
    assert.equal(calls.at(-1), 'DELETE v1/access-requests/r1')
    assert.equal(sent.length, answered ? 1 : 0)
  }
})

test('asking again waits for the withdrawn request to be gone, so the computer keeps the new one', async () => {
  calls.length = sent.length = 0
  posts = 0
  quick = false
  polls = [{ status: 'pending' }]
  const held = deferred()
  holdPost = held.promise
  const first = requestAccess()
  for (let i = 0; i < 100 && !calls.length; i++) await settle()
  handlers.get('cloud:cancelAccess')!({}, 'studio')
  const second = requestAccess()
  await settle()
  assert.deepEqual(steps(), ['POST computers/studio/access-requests'], 'the new request waits for the old one')
  held.resolve()
  holdPost = null
  polls = [{ status: 'approved' }]
  for (let i = 0; i < 100 && calls.length < 4; i++) await settle()
  assert.deepEqual(steps(), ['POST computers/studio/access-requests', 'DELETE access-requests/r1', 'POST computers/studio/access-requests', 'GET access-requests/r2'])
  assert.equal(await ending(first), 'cancelled')
  assert.equal(await ending(second), 'approved')
})
