import assert from 'node:assert/strict'
import { existsSync, readFileSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
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
Object.assign(globalThis, {
  localStorage: { getItem: () => null, setItem() {}, removeItem() {} },
  window: { addEventListener() {}, codync: {
    appVersion: '2.38.0',
    cloud: {
      request: async (method: string, path: string) => { cloudCalls.push(`${method} ${path}`); return { ok: true, value: {} } },
      requestAccess: () => new Promise<AccessTicket>((resolve) => { answerAccess = resolve }),
    },
  } },
})
// The store modules read `window` as they load, so they come in after it is set.
const { AccountComputers } = await import('../store/account-computers.ts')

const studio = { computerId: 'studio', name: 'Studio', signKey: 'key', access: 'none' }

test('a request withdrawn before the computer answers is taken off the computer too', async () => {
  const access = new AccountComputers({ setError() {} } as unknown as AppModel)
  const asking = access.ask(studio)
  access.cancel('studio')
  assert.deepEqual(cloudCalls, [], 'no ticket yet: nothing to delete')
  answerAccess({ requestId: 'r1', code: '123456', expiresAt: Date.now() + 60_000 })
  await asking
  await new Promise((r) => setImmediate(r))
  assert.deepEqual(cloudCalls, ['DELETE v1/access-requests/r1'])
  assert.equal(access.asking.has('studio'), false)
})
