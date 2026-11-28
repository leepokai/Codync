import { test, expect, type Page } from '@playwright/test'
import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { pinnedHostCall, verifySandbox, withLoopbackCloud, type BridgeCall, type PinnedHost } from './cloud-fixture.ts'
import { attach, sandbox } from './sandbox.ts'

/** The host id of the host the renderer is actually connected to, not just the sandbox's port. */
async function connectedHostId(page: Page): Promise<string> {
  const base = await page.evaluate(async () => {
    const host = (window as unknown as { codync: { host: { snapshot(): Promise<{ local?: { baseURL: string } }> } } }).codync.host
    return (await host.snapshot()).local?.baseURL ?? ''
  })
  if (!base) throw new Error('the app is not connected to a host')
  const response = await fetch(`${base}/health`, { signal: AbortSignal.timeout(5_000) })
  if (!response.ok) throw new Error(`health -> ${response.status}`)
  return ((await response.json()) as { hostId?: string }).hostId ?? ''
}

/** Captures the host the renderer is connected to, once; every later call goes to that same host. */
async function pinSession(page: Page, expectedHostId: string): Promise<PinnedHost> {
  const connected = await page.evaluate(async () => {
    const host = (window as unknown as { codync: { host: { snapshot(): Promise<{ local?: PinnedHost }> } } }).codync.host
    const local = (await host.snapshot()).local
    if (!local) throw new Error('the app is not connected to a host')
    return { baseURL: local.baseURL, token: local.token }
  })
  const response = await fetch(`${connected.baseURL}/health`, { signal: AbortSignal.timeout(5_000) })
  const health = response.ok ? ((await response.json()) as { hostId?: string }) : {}
  if (health.hostId !== expectedHostId) throw new Error(`the app is connected to host ${health.hostId ?? 'nothing'}, not ${expectedHostId}`)
  return connected
}

/** Calls the pinned host at its own base URL; it never selects a host again. */
function pinnedCall(page: Page, pin: PinnedHost) {
  const bridge: BridgeCall = {
    call: (baseURL, token, method, body, timeoutMs) =>
      page.evaluate(
        async ([base, key, name, params, timeout]) => {
          const host = (window as unknown as { codync: { host: { call(baseURL: string, token: string, method: string, body: unknown, timeoutMs: number): Promise<unknown> } } }).codync.host
          try {
            return await host.call(base, key, name, params, timeout)
          } catch (error) {
            // The bridge rejects with a plain `{ status, message }`; across `page.evaluate` that object
            // would read as "Object", so keep the host's own words.
            const message = (error as { message?: unknown }).message
            throw new Error(typeof message === 'string' ? message : String(error))
          }
        },
        [baseURL, token, method, body, timeoutMs] as const,
      ),
  }
  return <T>(method: string, params: Record<string, unknown>) => pinnedHostCall<T>(bridge, pin, method, params)
}
type Connector = { id: string; name: string; auth?: string }
type Refusal = { error: string; description: string }

/** A remote MCP server whose sign-in registration accepts only one loopback redirect URI, like Upwork (#440). */
async function loopbackOnlyServer(refusal?: Refusal) {
  const registrations: string[][] = []
  const server: Server = createServer((req, res) => {
    const base = `http://127.0.0.1:${(server.address() as AddressInfo).port}`
    const send = (status: number, body: unknown) => res.writeHead(status, { 'content-type': 'application/json' }).end(JSON.stringify(body))
    const url = new URL(req.url ?? '/', base)
    let raw = ''
    req.on('data', (chunk) => (raw += chunk))
    req.on('end', () => {
      if (url.pathname === '/mcp') return res.writeHead(401, { 'www-authenticate': 'Bearer' }).end()
      if (url.pathname === '/.well-known/oauth-authorization-server') {
        return send(200, { issuer: base, authorization_endpoint: `${base}/authorize`, token_endpoint: `${base}/token`, registration_endpoint: `${base}/register` })
      }
      if (url.pathname === '/register') {
        const uris = (JSON.parse(raw) as { redirect_uris: string[] }).redirect_uris
        registrations.push(uris)
        // A refusal about something else: the host must keep this reason and must not retry loopback-only.
        if (refusal) return send(400, { error: refusal.error, error_description: refusal.description })
        if (uris.length === 1 && uris[0].startsWith('http://127.0.0.1:')) return send(201, { client_id: 'loopback-only' })
        return send(400, { error: 'invalid_redirect_uri', error_description: 'only loopback redirect URIs are allowed' })
      }
      // The person approves at once: back to the redirect URI with a code.
      if (url.pathname === '/authorize') {
        const back = new URL(url.searchParams.get('redirect_uri') ?? '')
        back.searchParams.set('code', 'e2e-code')
        back.searchParams.set('state', url.searchParams.get('state') ?? '')
        return res.writeHead(302, { location: back.toString() }).end()
      }
      if (url.pathname === '/token') return send(200, { access_token: 'e2e-token', token_type: 'Bearer', expires_in: 3600 })
      send(404, {})
    })
  })
  await new Promise<void>((done) => server.listen(0, '127.0.0.1', done))
  return { server, registrations, url: `http://127.0.0.1:${(server.address() as AddressInfo).port}/mcp` }
}

test('signing in on the computer to a server that accepts only the loopback redirect URI works', async () => {
  // The sandbox must be the one this run may change; verifySandbox refuses another host or an imported config.
  // Read-only provenance first: attach writes a preference, so it must not run before this check.
  const box = { home: sandbox().home, port: sandbox().port }
  const verified = await verifySandbox(box)
  const { browser, page } = await attach()
  const pin = await pinSession(page, verified.hostId)
  const call = pinnedCall(page, pin)
  const mock = await loopbackOnlyServer()
  const name = `Loopback-only e2e ${Date.now()}`
  let id: string | undefined
  const cloud = `http://127.0.0.1:${(mock.server.address() as AddressInfo).port}`
  try {
    await withLoopbackCloud(call, { ...box, liveHostId: () => connectedHostId(page) }, cloud, async () => {
      id = (await call<{ connector: Connector }>('installConnector', { name, url: mock.url })).connector.id
      const plan = await call<{ url: string; callback?: string }>('connectorSignIn', { id, showsCode: true })
      const authorize = new URL(plan.url)
      expect(authorize.searchParams.get('client_id')).toBe('loopback-only')
      expect(authorize.searchParams.get('redirect_uri')).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/oauth\/callback$/)

      // The fallback only runs with the cloud on: two registrations, in this order — both URIs (refused),
      // then the loopback URI alone (accepted). With the cloud off this would be one and pass anyway.
      expect(mock.registrations.map((uris) => uris.length)).toEqual([2, 1])
      expect(mock.registrations[0][0]).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/oauth\/callback$/)
      expect(mock.registrations[0][1].startsWith(`${cloud}/v1/oauth/callback`)).toBe(true)
      expect(mock.registrations[1][0]).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/oauth\/callback$/)

      // The browser's part: the server sends it back to this host's loopback page, which finishes the sign-in.
      expect((await fetch(plan.url)).ok).toBe(true)
      await expect
        .poll(async () => (await call<{ items: Connector[] }>('connectors', {})).items.find((c) => c.id === id)?.auth)
        .toBe('signedIn')
    })
  } finally {
    if (id) await call('removeConnector', { id }).catch(() => {})
    mock.server.close()
    await browser.close()
  }
})

test('a refusal for another reason keeps its message and does not retry the loopback URI', async () => {
  // Read-only provenance first: attach writes a preference, so it must not run before this check.
  const box = { home: sandbox().home, port: sandbox().port }
  const verified = await verifySandbox(box)
  const { browser, page } = await attach()
  const pin = await pinSession(page, verified.hostId)
  const call = pinnedCall(page, pin)
  const refusal = { error: 'invalid_client_metadata', description: 'missing access token' }
  const mock = await loopbackOnlyServer(refusal)
  const name = `Refused e2e ${Date.now()}`
  let id: string | undefined
  const cloud = `http://127.0.0.1:${(mock.server.address() as AddressInfo).port}`
  try {
    await withLoopbackCloud(call, { ...box, liveHostId: () => connectedHostId(page) }, cloud, async () => {
      id = (await call<{ connector: Connector }>('installConnector', { name, url: mock.url })).connector.id
      await expect(call('connectorSignIn', { id, showsCode: true })).rejects.toThrow(/missing access token/)
      // Both URIs were tried once; the loopback-only retry is for a redirect refusal only.
      expect(mock.registrations).toHaveLength(1)
      expect(mock.registrations[0]).toHaveLength(2)
      const after = (await call<{ items: Connector[] }>('connectors', {})).items.find((c) => c.id === id)
      expect(after?.auth).not.toBe('signedIn')
    })
  } finally {
    if (id) await call('removeConnector', { id }).catch(() => {})
    mock.server.close()
    await browser.close()
  }
})
