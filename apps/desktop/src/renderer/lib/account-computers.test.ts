import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { mock, test } from 'node:test'
import ts from 'typescript'
import type { AccessTicket } from '@shared/ipc'
import type { AppModel } from '../store/app-model'

registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('@shared/')) return nextResolve(new URL(`../../shared/${specifier.slice(8)}.ts`, import.meta.url).href, context)
  if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
    const url = new URL(`${specifier}.ts`, context.parentURL)
    if (existsSync(url)) return nextResolve(url.href, context)
  }
  return nextResolve(specifier, context)
}, load(url, context, nextLoad) {
  if (url.endsWith('.ts')) return { format: 'module', shortCircuit: true, source: ts.transpileModule(readFileSync(new URL(url), 'utf8'), { compilerOptions: { target: ts.ScriptTarget.ESNext, module: ts.ModuleKind.ESNext } }).outputText }
  return nextLoad(url, context)
} })

const cloudCalls: string[] = []
let answerAccess!: (ticket: AccessTicket) => void
/** How the cloud answers a request's status poll. */
let status: () => Promise<{ status: string }> = async () => ({ status: 'pending' })
Object.assign(globalThis, {
  localStorage: { getItem: () => null, setItem() {}, removeItem() {} },
  window: { addEventListener() {}, codync: {
    appVersion: '2.38.0',
    cloud: {
      request: async (method: string, path: string) => {
        if (method === 'GET') return { ok: true, value: await status() }
        cloudCalls.push(`${method} ${path}`)
        return { ok: true, value: {} }
      },
      requestAccess: () => new Promise<AccessTicket>((resolve) => { answerAccess = resolve }),
      cancelAccess: (computerId: string) => cloudCalls.push(`cancel ${computerId}`),
    },
  } },
})
// The store modules read `window` as they load, so they come in after it is set.
const { AccountComputers } = await import('../store/account-computers.ts')
const { analytics } = await import('../store/analytics.ts')
const events: unknown[] = []
analytics.connect(async (e) => void events.push(e))

const studio = { computerId: 'studio', name: 'Studio', signKey: 'key', access: 'none' }
const settle = () => new Promise((r) => setImmediate(r))
const ticket = (requestId: string, expiresAt = Date.now() + 60_000) => ({ requestId, code: '123456', expiresAt })

/** A request the computer answered with its code, its status polled every 2 s on a mocked clock. */
async function asked(requestId: string, expiresAt?: number) {
  mock.timers.reset()
  mock.timers.enable({ apis: ['setTimeout'] })
  events.length = 0
  const access = new AccountComputers({ setError() {} } as unknown as AppModel)
  const asking = access.ask(studio)
  answerAccess(ticket(requestId, expiresAt))
  await settle()
  const poll = async () => { mock.timers.tick(2_000); await settle() }
  return { access, asking, poll }
}

test('a request withdrawn before the computer answers is taken off the computer too', async () => {
  const access = new AccountComputers({ setError() {} } as unknown as AppModel)
  const asking = access.ask(studio)
  access.cancel('studio')
  assert.deepEqual(cloudCalls, ['cancel studio'], 'no ticket yet: the main process withdraws the request it holds')
  // The code was already on its way when Cancel reached the main process.
  answerAccess(ticket('r1'))
  await asking
  await settle()
  assert.deepEqual(cloudCalls, ['cancel studio', 'DELETE v1/access-requests/r1'])
  assert.equal(access.asking.has('studio'), false)
})

test('a request counts once asked, and its end counts with why: denied, expired or cancelled', async (t) => {
  t.after(() => mock.timers.reset())
  status = async () => ({ status: 'denied' })
  const denied = await asked('r2')
  await denied.poll()
  await denied.asking
  assert.deepEqual(events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'denied' }])

  const expired = await asked('r3', Date.now() - 1)
  await expired.poll()
  await expired.asking
  assert.deepEqual(events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'expired' }])

  const cancelled = await asked('r4')
  cancelled.access.cancel('studio')
  await cancelled.poll()
  await cancelled.asking
  assert.deepEqual(events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'cancelled' }])
})

test('a request cancelled while its status is on the way counts as cancelled only', async (t) => {
  t.after(() => mock.timers.reset())
  let answer!: (s: { status: string }) => void
  status = () => new Promise((resolve) => { answer = resolve })
  const { access, asking, poll } = await asked('r5')
  await poll()
  access.cancel('studio')
  answer({ status: 'denied' })
  await asking
  await settle()
  assert.deepEqual(events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'cancelled' }])
})
