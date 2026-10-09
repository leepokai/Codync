import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'
import type { AppModel } from '../store/app-model'

registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('@shared/')) return nextResolve(new URL(`../../shared/${specifier.slice(8)}.ts`, import.meta.url).href, context)
  if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
    const url = new URL(`${specifier}.ts`, context.parentURL)
    if (existsSync(url)) return nextResolve(url.href, context)
  }
  return nextResolve(specifier, context)
} })

test('repeated checks never request permissions; explicit actions and failures stay reviewable', async () => {
  const calls: string[] = []
  let fail = false
  let capture = false
  let settings = ''
  Object.assign(globalThis, { window: { codync: { platform: 'darwin', app: {
    screenAgentState: async () => ({ available: true, needsApproval: false, registered: true }),
    openSettings: (url: string) => { settings = url },
  } } } })
  const app = { local: { connection: { kind: 'online' }, client: { call: async (method: string) => {
    calls.push(method)
    if (method === 'requestScreenPermission' && fail) throw new Error('Permission denied')
    return { enabled: true, connected: true, capture, input: false, permissionApp: 'CodyncDevScreen' }
  } } }, setRemoteScreen: async () => { calls.push('enable') }, screenError: null } as unknown as AppModel
  const { ComputerAccess } = await import('../store/computer-access.ts')
  const access = new ComputerAccess(app)
  await access.refresh()
  await access.refresh()
  assert.deepEqual(calls, ['screenStatus', 'screenStatus'])
  assert.equal(access.helperName, 'CodyncDevScreen')
  assert.equal(access.step, 'capture')
  access.openSettings('input')
  assert.match(settings, /Privacy_Accessibility/)
  assert.equal(access.step, 'capture')
  fail = true
  await access.request('capture')
  assert.equal(access.busy, null)
  assert.equal(access.error, 'Permission denied')
  assert.equal(access.requested.has('capture'), true)
  fail = false
  capture = true
  await access.request('capture')
  assert.equal(access.error, null)
  assert.equal(access.step, 'input')
  await access.enable()
  assert.ok(calls.includes('enable'))
})
