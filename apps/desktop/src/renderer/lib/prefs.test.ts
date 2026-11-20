import assert from 'node:assert/strict'
import { test } from 'node:test'
import { setMaxListeners } from 'node:events'

test('A stored preference of another kind reads as the fallback', async () => {
  const stored = new Map<string, string>([
    ['hiddenComputers', 'null'],
    ['computerOrder', '{"a":1}'],
    ['sidebarCompact', '"yes"'],
    ['desktopSidebarWidth', '300'],
    ['computerAccessSetup', '"ready"'],
  ])
  const events = new EventTarget()
  setMaxListeners(0, events)
  Object.defineProperty(globalThis, 'window', { configurable: true, value: events })
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: { getItem: (key: string) => stored.get(key) ?? null, setItem: (key: string, value: string) => void stored.set(key, value) },
  })
  const { prefs } = await import('./prefs.ts')
  assert.equal(prefs.hiddenComputers.get(), '')
  assert.deepEqual(prefs.computerOrder.get(), [])
  assert.equal(prefs.sidebarCompact.get(), false)
  assert.equal(prefs.sidebarWidth.get(), 300)
  assert.equal(prefs.computerAccessSetup.get(), 'ready')
})
