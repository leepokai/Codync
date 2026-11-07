import assert from 'node:assert/strict'
import { test } from 'node:test'
import { loadMainModule } from './test-support/main-module.ts'
import type { CodyncBridge } from './ipc'

/** One page load of the preload: what it sent the main process, its `window.codync`, and its unload. */
function loadPage() {
  const sent: unknown[][] = []
  const listeners = new Map<string, () => void>()
  let bridge!: CodyncBridge
  const ipcRenderer = {
    on() {},
    send: (...args: unknown[]) => void sent.push(args),
    sendSync: () => null,
    invoke: async () => null,
  }
  loadMainModule('../preload/index', {
    electron: { ipcRenderer, contextBridge: { exposeInMainWorld: (_name: string, value: CodyncBridge) => void (bridge = value) } },
    './ssh': { sshBridge: {} },
  }, { window: { addEventListener: (name: string, fn: () => void) => listeners.set(name, fn) }, crypto, process })
  return { sent, bridge, unload: () => listeners.get('pagehide')?.() }
}

const computer = { id: 'c1', signKey: 'k' }

test("a reloaded page closes its relay streams, and the next page's streams never take their ids", () => {
  const before = loadPage()
  before.bridge.remote.stream(computer, 'events', {}, () => {}, () => {})
  before.bridge.host.stream('http://127.0.0.1:1/api/events', 't', () => {}, () => {})
  before.unload()
  const id = before.sent.find((m) => m[0] === 'remote:stream:open')?.[1]
  assert.ok(before.sent.some((m) => m[0] === 'remote:stream:close' && m[1] === id), 'the relay subscription closes')
  assert.ok(before.sent.some((m) => m[0] === 'host:stream:close'), 'the local stream closes')

  const after = loadPage()
  after.bridge.remote.stream(computer, 'events', {}, () => {}, () => {})
  assert.notEqual(after.sent.find((m) => m[0] === 'remote:stream:open')?.[1], id)
})
