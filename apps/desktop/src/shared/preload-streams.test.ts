import assert from 'node:assert/strict'
import { test } from 'node:test'
import { loadMainModule } from './test-support/main-module.ts'
import type { CodyncBridge } from './ipc'

/** One page load of the preload: what it sent the main process, its `window.codync`, and its unload. */
function loadPage() {
  const sent: unknown[][] = []
  const invoked: unknown[][] = []
  const ipcListeners = new Map<string, ((e: unknown, ...args: unknown[]) => void)[]>()
  const listeners = new Map<string, () => void>()
  let bridge!: CodyncBridge
  const ipcRenderer = {
    on: (channel: string, fn: (e: unknown, ...args: unknown[]) => void) => void ipcListeners.set(channel, [...(ipcListeners.get(channel) ?? []), fn]),
    removeListener() {},
    send: (...args: unknown[]) => void sent.push(args),
    sendSync: () => null,
    invoke: (...args: unknown[]) => new Promise(() => void invoked.push(args)),
  }
  /** A message from the main process. */
  const receive = (channel: string, ...args: unknown[]) => ipcListeners.get(channel)?.forEach((fn) => fn({}, ...args))
  loadMainModule('../preload/index', {
    electron: { ipcRenderer, contextBridge: { exposeInMainWorld: (_name: string, value: CodyncBridge) => void (bridge = value) } },
    './ssh': { sshBridge: {} },
  }, { window: { addEventListener: (name: string, fn: () => void) => listeners.set(name, fn) }, crypto, process })
  return { sent, invoked, receive, bridge, unload: () => listeners.get('pagehide')?.() }
}

test("a reloaded page closes its relay streams, and the next page's streams never take their ids", () => {
  const before = loadPage()
  before.bridge.channel.stream('c1', 'k', '/events', () => {}, () => {})
  before.bridge.host.stream('http://127.0.0.1:1/api/events', 't', () => {}, () => {})
  before.unload()
  const id = before.sent.find((m) => m[0] === 'channel:stream:open')?.[1]
  assert.ok(before.sent.some((m) => m[0] === 'channel:stream:close' && m[1] === id), 'the relay subscription closes')
  assert.ok(before.sent.some((m) => m[0] === 'host:stream:close'), 'the local stream closes')

  const after = loadPage()
  after.bridge.channel.stream('c1', 'k', '/events', () => {}, () => {})
  assert.notEqual(after.sent.find((m) => m[0] === 'channel:stream:open')?.[1], id)
})

test("a code meant for a request withdrawn earlier doesn't show on the next one for the same computer", () => {
  const page = loadPage()
  const computer = { computerId: 'c1', signKey: 'k' }
  const codes: string[] = []
  void page.bridge.cloud.requestAccess(computer, (code) => codes.push(`first ${code}`))
  page.bridge.cloud.cancelAccess('c1')
  void page.bridge.cloud.requestAccess(computer, (code) => codes.push(`second ${code}`))
  const [first, second] = page.invoked.filter((args) => args[0] === 'cloud:requestAccess').map((args) => args[2])
  page.receive('cloud:accessCode', first, '111111')
  page.receive('cloud:accessCode', second, '222222')
  assert.deepEqual(codes, ['first 111111', 'second 222222'])
})
