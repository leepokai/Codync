import assert from 'node:assert/strict'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { pinnedHostCall, verifySandbox, withLoopbackCloud, type CloudStatus, type Sandbox } from './cloud-fixture.ts'

const sandbox: Sandbox = { home: 'C:/sandbox/home', port: 19301 }
const noVerify = async () => ({ hostId: 'h1' })
const stat = () => ({ isDirectory: () => true })
const healthOf = (hostId?: string) => async () => ({ hostId })

/** A real `codync.db` with the host's schema (`kv(k, v)`), written to a temporary folder. */
function realDb(rows: Record<string, string>): string {
  const dir = mkdtempSync(join(tmpdir(), 'cloud-fixture-'))
  const { DatabaseSync } = process.getBuiltinModule('node:sqlite') as typeof import('node:sqlite')
  const db = new DatabaseSync(join(dir, 'codync.db'))
  db.exec('CREATE TABLE kv (k TEXT PRIMARY KEY, v TEXT NOT NULL)')
  const insert = db.prepare('INSERT INTO kv (k, v) VALUES (?, ?)')
  for (const [key, value] of Object.entries(rows)) insert.run(key, value)
  db.close()
  return dir
}

test('a real database with a saved relay token is refused (cloud_relay_token, not cloud_token)', async () => {
  for (const key of ['cloud_url', 'cloud_relay_token']) {
    const home = realDb({ host_id: 'h1', [key]: 'x' })
    await assert.rejects(
      () => verifySandbox({ home, port: 1 }, { stat, health: healthOf('h1'), env: () => [] }),
      new RegExp(`already holds ${key}`),
    )
  }
})

test('the cloud switch alone is not a saved configuration: a --cloud sandbox starts with it on or off', async () => {
  for (const value of ['true', 'false']) {
    const home = realDb({ host_id: 'h1', cloud_enabled: value })
    const result = await verifySandbox({ home, port: 1 }, { stat, health: healthOf('h1'), env: () => [] })
    assert.equal(result.hostId, 'h1')
  }
})

test('a real empty database whose host id matches health passes', async () => {
  const home = realDb({ host_id: 'h1' })
  const result = await verifySandbox({ home, port: 1 }, { stat, health: healthOf('h1'), env: () => [] })
  assert.equal(result.hostId, 'h1')
})

test('a cloud URL or token in the child environment is refused, presence only', async () => {
  for (const key of ['CODYNC_CLOUD_URL', 'CODYNC_CLOUD_TOKEN']) {
    await assert.rejects(
      () => verifySandbox(sandbox, { stat, hostKeys: () => ({ hostId: 'h1', cloudKeys: [], enabled: false }), health: healthOf('h1'), env: () => [key] }),
      /environment sets/,
    )
  }
})

function fakeHost(initial: CloudStatus, options: { restoreFails?: boolean } = {}) {
  let status: CloudStatus = { ...initial }
  const calls: string[] = []
  const call = async (method: string, params: Record<string, unknown>) => {
    calls.push(method)
    if (method === 'cloudStatus') return { ...status }
    if (method === 'setCloudUrl') {
      if (options.restoreFails && params.url === '') {
        status = { ...status, enabled: true }
        throw new Error('cannot clear the URL')
      }
      status = { ...status, url: params.url as string }
      return status
    }
    if (method === 'setCloud') {
      status = { ...status, enabled: params.enabled as boolean }
      return status
    }
    throw new Error(`unexpected ${method}`)
  }
  return { call, calls, status: () => status }
}

const fresh: CloudStatus = { enabled: false }

test('a verified sandbox is used and put back', async () => {
  const host = fakeHost(fresh)
  const seen = await withLoopbackCloud(host.call, sandbox, 'http://127.0.0.1:9', async () => 'ok', noVerify)
  assert.equal(seen, 'ok')
  assert.deepEqual(host.calls, ['cloudStatus', 'setCloudUrl', 'setCloud', 'setCloud', 'setCloudUrl', 'cloudStatus'])
  assert.deepEqual(host.status(), { enabled: false, url: '' })
})

test('an app connected to another host is refused before anything changes', async () => {
  const host = fakeHost(fresh)
  await assert.rejects(
    () => withLoopbackCloud(host.call, { ...sandbox, liveHostId: async () => 'other' }, 'http://127.0.0.1:9', async () => 'ok', noVerify),
    /connected to host other/,
  )
  assert.deepEqual(host.calls, [])
})

for (const [name, status] of [
  ['an account', { enabled: false, owner: { userId: 'u' } }],
  ['a saved token', { enabled: false, tokenSet: true }],
] as const) {
  test(`${name} is refused before anything changes`, async () => {
    const host = fakeHost(status as CloudStatus)
    await assert.rejects(
      () => withLoopbackCloud(host.call, sandbox, 'http://127.0.0.1:9', async () => 'ok', noVerify),
      /dedicated sandbox/,
    )
    assert.deepEqual(host.calls, ['cloudStatus'])
  })
}

test('a sandbox that started with the cloud on is put back on', async () => {
  const host = fakeHost({ enabled: true, url: 'https://dev.example' })
  const seen = await withLoopbackCloud(host.call, sandbox, 'http://127.0.0.1:9', async () => 'ok', noVerify)
  assert.equal(seen, 'ok')
  assert.deepEqual(host.calls, ['cloudStatus', 'setCloudUrl', 'setCloud', 'setCloud', 'setCloudUrl', 'setCloud', 'cloudStatus'])
  assert.equal(host.status().enabled, true)
})

test('a run failure is kept even when cleanup also fails', async () => {
  const host = fakeHost(fresh, { restoreFails: true })
  const failure = await withLoopbackCloud(host.call, sandbox, 'http://127.0.0.1:9', async () => {
    throw new Error('boom')
  }, noVerify).catch((error: unknown) => error)
  assert.ok(failure instanceof AggregateError, `${failure}`)
  assert.match(String((failure as AggregateError).errors[0]), /boom/)
  assert.match((failure as Error).message, /cleanup failed/)
})

test('a cleanup that leaves the cloud on is not swallowed', async () => {
  const host = fakeHost(fresh)
  const call = async (method: string, params: Record<string, unknown>) => {
    if (method === 'setCloud' && params.enabled === false) return undefined // pretends to turn it off
    return host.call(method, params)
  }
  await assert.rejects(
    () => withLoopbackCloud(call, sandbox, 'http://127.0.0.1:9', async () => 'ok', noVerify),
    /cleanup failed/,
  )
})

test('a host that changes after the URL write is never written again, and cleanup is reported', async () => {
  let host = 'h1'
  const calls: string[] = []
  const call = async (method: string) => {
    calls.push(method)
    if (method === 'cloudStatus') return { enabled: false }
    if (method === 'setCloudUrl') {
      host = 'h2'
      return { enabled: true }
    }
    return {}
  }
  const failure = await withLoopbackCloud(
    call,
    { ...sandbox, liveHostId: async () => host },
    'http://127.0.0.1:9',
    async () => 'ok',
    noVerify,
  ).catch((error: unknown) => error)
  assert.ok(failure instanceof AggregateError, `${failure}`)
  assert.match((failure as Error).message, /cleanup failed/)
  // Only the first write reached the original host; the second write and both cleanup writes were refused.
  assert.deepEqual(calls, ['cloudStatus', 'setCloudUrl', 'cloudStatus'])
})
test('a pinned call always reaches the host captured first, even after the app reconnects', async () => {
  const reached: string[] = []
  let selected = 'A'
  const bridge = {
    call: async (baseURL: string) => {
      reached.push(`${baseURL}/${selected}`)
      // The app has reconnected to B by now; the pinned call must not follow it.
      selected = 'B'
      return {}
    },
  }
  await pinnedHostCall(bridge, { baseURL: 'http://a', token: 't' }, 'cloudStatus', {})
  await pinnedHostCall(bridge, { baseURL: 'http://a', token: 't' }, 'setCloudUrl', { url: 'x' })
  assert.deepEqual(reached, ['http://a/A', 'http://a/B'])
})
