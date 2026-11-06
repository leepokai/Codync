import assert from 'node:assert/strict'
import { existsSync } from 'node:fs'
import { registerHooks } from 'node:module'
import { test } from 'node:test'

// Run the actual renderer store with a browser's local storage and account events.
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
      const url = new URL(`${specifier}.ts`, context.parentURL)
      if (existsSync(url)) return nextResolve(url.href, context)
    }
    return nextResolve(specifier, context)
  },
})

test('computer ordering stays on each device, survives restart, and isolates accounts', async () => {
  let disk = new Map<string, string>()
  const desktopDisk = disk
  const listeners = new Map<string, ((event: unknown) => void)[]>()
  const user = { userId: 'same-account', email: null, avatarURL: null }
  let session = { user: user as typeof user | null, cloudURL: 'https://cloud.test', ready: true, configured: true, busy: false, error: null }
  const network = () => assert.fail('Ordering must not send network requests')
  Object.assign(globalThis, {
    localStorage: { getItem: (key: string) => disk.get(key) ?? null, setItem: (key: string, value: string) => disk.set(key, value) },
    window: {
      addEventListener: (type: string, callback: (event: unknown) => void) => listeners.set(type, [...listeners.get(type) ?? [], callback]),
      setInterval: () => assert.fail('Ordering must not poll'),
      codync: { cloud: { request: network }, account: { state: async () => session, onChange() {} } },
    },
    fetch: network,
  })
  const { account } = await import('../store/account.ts')
  const { DeviceComputerOrder } = await import('../store/computer-order.ts')
  const selectAccount = async (id: string | null, cloudURL = 'https://cloud.test') => {
    session = { ...session, user: id ? { ...user, userId: id } : null, cloudURL }
    await account.start()
    await new Promise((resolve) => setImmediate(resolve))
  }
  await selectAccount('same-account')
  const key = 'computerOrder.https://cloud.test/same-account'
  desktopDisk.set(key, JSON.stringify({ ids: ['a', 'hidden', 'b'], pending: true }))
  const desktop = new DeviceComputerOrder()
  assert.deepEqual(desktop.ids, ['a', 'hidden', 'b'])
  desktop.move('b', 'a', ['a', 'b', 'new'])
  assert.deepEqual(JSON.parse(desktopDisk.get(key)!), { ids: ['b', 'a', 'hidden', 'new'] })
  assert.deepEqual(new DeviceComputerOrder().ids, desktop.ids)

  // A second installation uses the same account but has a separate localStorage.
  disk = new Map()
  const laptop = new DeviceComputerOrder()
  assert.deepEqual(laptop.ids, [])
  laptop.move('a', 'b', ['a', 'b'])
  assert.deepEqual(laptop.ids, ['b', 'a'])
  assert.deepEqual(desktop.ids, ['b', 'a', 'hidden', 'new'])
  assert.deepEqual(new DeviceComputerOrder().ids, ['b', 'a'])
  assert.deepEqual(JSON.parse(desktopDisk.get(key)!).ids, desktop.ids)

  disk = desktopDisk
  await selectAccount('another-account')
  assert.deepEqual(desktop.ids, [])
  desktop.move('x', 'y', ['x', 'y'])
  await selectAccount('same-account')
  assert.deepEqual(desktop.ids, ['b', 'a', 'hidden', 'new'])
  await selectAccount('same-account', 'https://another-cloud.test')
  assert.deepEqual(desktop.ids, [])
  await selectAccount(null)
  desktop.move('local', 'ssh', ['local', 'ssh'])
  assert.deepEqual(JSON.parse(desktopDisk.get('computerOrder')!), ['ssh', 'local'])
  assert.equal(listeners.has('focus'), false)
  assert.equal(listeners.has('online'), false)
})
