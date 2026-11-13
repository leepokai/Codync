import { test, expect } from '@playwright/test'
import { createServer, type Server } from 'node:http'
import type { AddressInfo } from 'node:net'
import { api, attach } from './sandbox.ts'

type Connector = { id: string; name: string; auth?: string }

/** A remote MCP server whose sign-in registration accepts only one loopback redirect URI, like Upwork (#440). */
async function loopbackOnlyServer() {
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
  const { browser, page } = await attach()
  const mock = await loopbackOnlyServer()
  const name = `Loopback-only e2e ${Date.now()}`
  let id: string | undefined
  try {
    id = (await api<{ connector: Connector }>(page, 'installConnector', { name, url: mock.url })).connector.id
    const plan = await api<{ url: string; callback?: string }>(page, 'connectorSignIn', { id, showsCode: true })
    const authorize = new URL(plan.url)
    expect(authorize.searchParams.get('client_id')).toBe('loopback-only')
    expect(authorize.searchParams.get('redirect_uri')).toMatch(/^http:\/\/127\.0\.0\.1:\d+\/oauth\/callback$/)
    // Never more than the both-URIs try and the loopback-only one.
    expect(mock.registrations.at(-1)).toHaveLength(1)
    expect(mock.registrations.length).toBeLessThanOrEqual(2)

    // The browser's part: the server sends it back to this host's loopback page, which finishes the sign-in.
    expect((await fetch(plan.url)).ok).toBe(true)
    await expect
      .poll(async () => (await api<{ items: Connector[] }>(page, 'connectors', {})).items.find((c) => c.id === id)?.auth)
      .toBe('signedIn')
  } finally {
    if (id) await api(page, 'removeConnector', { id }).catch(() => {})
    mock.server.close()
    await browser.close()
  }
})
