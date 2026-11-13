import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import ts from 'typescript'
import type { AccessResult } from '@shared/ipc'
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

const cancelled: string[] = []
/** The main process's request: `showCode` as the computer shows it, `end` with how it ended, `fail` as the IPC rejects. */
let showCode!: (code: string) => void
let end!: (result: AccessResult) => void
let fail!: (error: Error) => void
let requests = 0
Object.assign(globalThis, {
  localStorage: { getItem: () => null, setItem() {}, removeItem() {} },
  window: { addEventListener() {}, codync: {
    appVersion: '2.38.0',
    cloud: {
      requestAccess: (_computer: unknown, onCode: (code: string) => void) => new Promise<AccessResult>((resolve, reject) => {
        requests += 1
        showCode = onCode
        end = resolve
        fail = reject
      }),
      cancelAccess: (computerId: string) => cancelled.push(computerId),
    },
  } },
})
// The store modules read `window` as they load, so they come in after it is set.
const { AccountComputers } = await import('../store/account-computers.ts')
const { analytics } = await import('../store/analytics.ts')
const events: unknown[] = []
analytics.connect(async (e) => void events.push(e))

const studio = { computerId: 'studio', name: 'Studio', signKey: 'key', access: 'none' }

/** A request whose code the computer showed, then ended `status`; what the person saw and what was counted. */
async function ended(status: string) {
  events.length = 0
  const errors: (string | null)[] = []
  const access = new AccountComputers({ setError: (e: string | null) => errors.push(e), cloud: { refresh: async () => {} } } as unknown as AppModel)
  const asking = access.ask(studio)
  showCode('123456')
  assert.equal(access.asking.get('studio')?.code, '123456')
  end({ ok: true, value: { status } })
  await asking
  assert.equal(access.asking.has('studio'), false)
  return { events: [...events], errors }
}

test('a request counts once the code shows, and its end counts with why: denied or expired', async () => {
  assert.deepEqual(await ended('approved'), { events: [{ event: 'access_requested' }], errors: [] })
  const denied = await ended('denied')
  assert.deepEqual(denied.events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'denied' }])
  assert.deepEqual(denied.errors, ['The computer declined the request.'])
  assert.deepEqual((await ended('expired')).events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'expired' }])
})

test('Cancel withdraws the request and counts as cancelled only, whatever ends it next', async () => {
  events.length = 0
  const access = new AccountComputers({ setError() { assert.fail('a cancelled request shows no error') } } as unknown as AppModel)
  const asking = access.ask(studio)
  showCode('123456')
  access.cancel('studio')
  assert.deepEqual(cancelled, ['studio'])
  end({ ok: true, value: { status: 'denied' } })
  await asking
  assert.deepEqual(events, [{ event: 'access_requested' }, { event: 'access_request_failed', reason: 'cancelled' }])
})

test("a code still on its way when the request was withdrawn doesn't count", async () => {
  events.length = 0
  const access = new AccountComputers({ setError() {} } as unknown as AppModel)
  void access.ask(studio)
  const lateCode = showCode
  access.cancel('studio')
  lateCode('111111')
  assert.deepEqual(events, [])
  end({ ok: true, value: { status: 'cancelled' } })
})

test('a request whose IPC fails shows why and can be asked again', async () => {
  const errors: (string | null)[] = []
  const access = new AccountComputers({ setError: (e: string | null) => errors.push(e) } as unknown as AppModel)
  const asking = access.ask(studio)
  fail(new Error('The app closed the request.'))
  await asking
  assert.equal(access.asking.has('studio'), false)
  assert.deepEqual(errors, ['The app closed the request.'])
  const before = requests
  void access.ask(studio)
  assert.equal(requests, before + 1)
  end({ ok: true, value: { status: 'cancelled' } })
})
