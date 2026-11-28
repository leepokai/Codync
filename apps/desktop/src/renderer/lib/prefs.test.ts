import assert from 'node:assert/strict'
import { test } from 'node:test'
import { setMaxListeners } from 'node:events'

test('A stored icon size the slider cannot show reads back on its grid', async () => {
  // This test binds storage first, so the map also carries the seeds the stored-pref test below
  // reads: the module loads once against this map and keeps those initial values for that test.
  const stored = new Map<string, string>([
    ['rosterIconSize', '31'],
    ['hiddenComputers', 'null'],
    ['computerOrder', '{"a":1}'],
    ['listHidden', '"yes"'],
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
  // Whatever another window or an older build left behind reads as a value the control can show,
  // and a write through the pref is normalized the same way before it is ever stored.
  assert.equal(prefs.rosterIconSize.get(), 32)
  prefs.rosterIconSize.set(12)
  assert.equal(prefs.rosterIconSize.get(), 24)
  prefs.rosterIconSize.set(99)
  assert.equal(prefs.rosterIconSize.get(), 44)
  prefs.rosterIconSize.set(31)
  assert.equal(prefs.rosterIconSize.get(), 32)
})

test('A stored preference of another kind reads as the fallback', async () => {
  const stored = new Map<string, string>([
    ['hiddenComputers', 'null'],
    ['computerOrder', '{"a":1}'],
    ['listHidden', '"yes"'],
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
  assert.equal(prefs.listHidden.get(), false)
  assert.equal(prefs.sidebarWidth.get(), 300)
  assert.equal(prefs.computerAccessSetup.get(), 'ready')
})

test('A roster icon size stays a whole number inside its range, on the slider step grid', async () => {
  const { clampRosterIconSize } = await import('./prefs.ts')
  assert.equal(clampRosterIconSize(30), 30)
  assert.equal(clampRosterIconSize(12), 24)
  assert.equal(clampRosterIconSize(99), 44)
  assert.equal(clampRosterIconSize(31.6), 32)
  // An odd whole number lands between the slider's steps: it snaps up to the grid, not off it.
  assert.equal(clampRosterIconSize(31), 32)
  assert.equal(clampRosterIconSize(31.4), 32)
  // The edges are pinned too: the odd top edge snaps up (a snap-to-floor bug would read 42),
  // and a non-default even value is a fixed point.
  assert.equal(clampRosterIconSize(43), 44)
  assert.equal(clampRosterIconSize(38), 38)
  assert.equal(clampRosterIconSize(Number.NaN), 30)
})
