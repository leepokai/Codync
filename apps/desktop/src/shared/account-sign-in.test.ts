import assert from 'node:assert/strict'
import { EventEmitter } from 'node:events'
import path from 'node:path'
import { test } from 'node:test'
import { AuthenticationEnded, BrowserAuthentication } from '../main/browser-authentication.ts'
import { loadMainModule } from './test-support/main-module.ts'

const opened: string[] = []
const revoked: string[] = []
const dropped: string[] = []
const retries: (() => void)[] = []
const platform = { open: async (url: string) => { opened.push(url) } }
const browser = new BrowserAuthentication((url) => platform.open(url))
const handleURL = (url: string) => browser.handleURL(url)
const { AccountService } = loadMainModule<typeof import('../main/account.ts')>('account', {
  'node:events': { EventEmitter }, 'node:fs': { promises: {} }, 'node:path': path,
  electron: { app: { getVersion: () => '2.33.1' }, ipcMain: {}, safeStorage: {} },
  './auth': { authenticate: browser.authenticate.bind(browser), AuthenticationEnded },
  './cloud': {
    revokeThisDevice: async (_base: string, token: string, userId: string) => { revoked.push(`${userId} ${token}`) },
    dropDeviceKey: async (userId: string) => { dropped.push(userId) },
  },
  './environment': {
    identity: { appId: 'test.codync', scheme: 'codync', environment: 'dev' },
    buildConfig: { clerkPublishableKey: `pk_test_${Buffer.from('clerk.example.test$').toString('base64')}`, cloudURL: 'https://cloud.example.test' },
  },
}, {
  URL, URLSearchParams, Buffer, process: { env: {}, platform: 'win32' }, fetch: (...args: Parameters<typeof fetch>) => globalThis.fetch(...args),
  setTimeout: (retry: () => void) => { retries.push(retry) },
})
const until = async (check: () => boolean) => {
  for (let i = 0; i < 50 && !check(); i++) await new Promise<void>((done) => setImmediate(done))
  assert.ok(check())
}
const response = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status })

test('closing the external browser leaves the app usable; retry and a stale nonce do not consume the current sign-in', async () => {
  opened.length = 0
  const account = new AccountService()
  const user = () => account.state.user
  let creates = 0
  const verified: string[] = []
  globalThis.fetch = async (input) => {
    const url = new URL(String(input))
    if (url.pathname.endsWith('/prepare_first_factor')) return response({ response: { id: `login-${creates}`, first_factor_verification: { external_verification_redirect_url: `https://browser.example.test/${creates}` } } })
    if (url.pathname.endsWith('/sign_ins')) return response({ response: { id: `login-${++creates}` } })
    if (url.pathname.includes('/sign_ins/')) {
      verified.push(url.pathname)
      if (url.searchParams.get('rotating_token_nonce') === 'incomplete') return response({ response: { id: `login-${creates}`, status: 'needs_identifier' }, client: { last_active_session_id: 'wrong-session', sessions: [{ id: 'wrong-session', status: 'active', user: { id: 'wrong-user' } }] } })
      if (url.searchParams.get('rotating_token_nonce') === 'stale') return response({ errors: [{ code: 'signed_out', message: 'Signed out' }] }, 401)
      return response({ response: { id: `login-${creates}`, status: 'complete' } })
    }
    return response({ response: { last_active_session_id: 'session', sessions: [{ id: 'session', status: 'active', user: { id: 'user' } }] } })
  }
  const first = account.signIn('apple')
  await until(() => opened.length === 1)
  assert.equal(account.state.busy, false)
  assert.equal(account.state.error, null)
  const second = account.signIn('google')
  await until(() => opened.length === 2)
  assert.equal(await first, 'cancelled')
  handleURL('test.codync://callback?rotating_token_nonce=stale')
  await until(() => verified.length === 1 && !account.state.busy)
  assert.equal(user(), null)
  assert.equal(account.state.error, null)
  handleURL('test.codync://callback?rotating_token_nonce=incomplete')
  await until(() => verified.length === 2 && !account.state.busy)
  assert.equal(user(), null)
  handleURL('test.codync://callback?rotating_token_nonce=current')
  assert.equal(await second, null)
  assert.equal(user()?.userId, 'user')
  assert.equal(account.state.busy, false)
  assert.deepEqual(verified, Array(3).fill('/v1/client/sign_ins/login-2'))
})

test('browser launch and callback network failures reset loading and allow a retry', async () => {
  const account = new AccountService()
  globalThis.fetch = async (input) => {
    const url = new URL(String(input))
    if (url.pathname.endsWith('/sign_ins')) return response({ response: { id: 'login' } })
    if (url.pathname.endsWith('/prepare_first_factor')) return response({ response: { id: 'login', first_factor_verification: { external_verification_redirect_url: 'https://browser.example.test' } } })
    throw new Error('Network unavailable')
  }
  platform.open = async () => { throw new Error('No browser') }
  assert.equal(await account.signIn('google'), 'error')
  assert.equal(account.state.busy, false)
  platform.open = async (url) => { opened.push(url) }
  opened.length = 0
  const retry = account.signIn('apple')
  await until(() => opened.length === 1)
  handleURL('test.codync://callback?rotating_token_nonce=current')
  assert.equal(await retry, 'error')
  assert.equal(account.state.busy, false)
  assert.match(account.state.error ?? '', /Couldn't sign in/)
})

test('deleting the account removes the user in Clerk; a failure changes nothing', async () => {
  const account = new AccountService()
  let deleted = false
  let reachable = false
  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input))
    if (url.pathname === '/v1/me' && init?.method === 'DELETE') {
      if (!reachable) throw new Error('Network unavailable')
      deleted = true
      return response({ response: { deleted: true }, client: { sessions: [] } })
    }
    return response({ response: deleted ? { sessions: [] } : { last_active_session_id: 'session', sessions: [{ id: 'session', status: 'active', user: { id: 'user' } }] } })
  }
  await (account as unknown as { refreshClient(): Promise<void> }).refreshClient()
  assert.equal(account.state.user?.userId, 'user')
  assert.equal(await account.deleteAccount(), false)
  assert.equal(account.state.user?.userId, 'user', 'still signed in')
  assert.equal(account.state.busy, false)
  assert.deepEqual(dropped, [])
  reachable = true
  assert.equal(await account.deleteAccount(), true)
  assert.equal(account.state.user, null)
  assert.equal(account.state.busy, false)
  assert.deepEqual(dropped, ['user'], "this install's key for the account goes")
  assert.deepEqual(revoked, [], "Clerk's user.deleted revokes it in the cloud")
})

const activeUser = { last_active_session_id: 'session', sessions: [{ id: 'session', status: 'active', user: { id: 'user' } }] }

test('signing out hands the key back only once Clerk signed out, on the token taken before', async () => {
  revoked.length = 0
  dropped.length = 0
  const account = new AccountService()
  let signedIn = true
  let reachable = false
  globalThis.fetch = async (input) => {
    const url = new URL(String(input))
    if (url.pathname === '/v1/client/sessions/session/tokens') return signedIn ? response({ jwt: 'jwt' }) : response({}, 404)
    if (url.pathname === '/v1/client/sessions/session/remove') {
      if (!reachable) throw new Error('Network unavailable')
      signedIn = false
      return response({ response: {}, client: { sessions: [] } })
    }
    return response({ response: signedIn ? activeUser : { sessions: [] } })
  }
  await (account as unknown as { refreshClient(): Promise<void> }).refreshClient()
  await account.signOut()
  assert.equal(account.state.user?.userId, 'user', 'still signed in')
  assert.match(account.state.error ?? '', /Couldn't sign out/)
  assert.deepEqual([revoked, dropped], [[], []], 'a sign-out that failed keeps the key')
  reachable = true
  await account.signOut()
  assert.equal(account.state.user, null)
  assert.equal(account.state.busy, false)
  assert.deepEqual(revoked, ['user jwt'])
  assert.deepEqual(dropped, ['user'])
})

test('a saved session stays restoring until Clerk answers; only Clerk turning it down signs out', async () => {
  const answers: (() => Response)[] = [
    () => response({ errors: [{ message: 'busy' }] }, 503),
    () => new Response('<html>Hotel Wi-Fi</html>', { status: 200 }),
    () => { throw new Error('Network unavailable') },
    () => response({ response: activeUser }),
  ]
  globalThis.fetch = async () => answers.shift()!()
  type Restoring = { deviceToken: string | null; restore(): Promise<void>; state: InstanceType<typeof AccountService>['state'] }
  const restoring = new AccountService() as unknown as Restoring
  restoring.deviceToken = 'saved'
  const restored = restoring.restore()
  for (let tries = 1; tries <= 3; tries++) {
    await until(() => retries.length === 1)
    assert.equal(restoring.state.ready, false, 'not shown signed out while Clerk can\'t say')
    retries.shift()!()
  }
  await restored
  assert.equal(restoring.state.ready, true)
  assert.equal(restoring.state.user?.userId, 'user')

  globalThis.fetch = async () => response({ errors: [{ message: 'Signed out' }] }, 401)
  const declined = new AccountService() as unknown as Restoring
  declined.deviceToken = 'saved'
  await declined.restore()
  assert.deepEqual([declined.state.ready, declined.state.user], [true, null])
})
