import assert from 'node:assert/strict'
import test from 'node:test'
import { cloudStatusLine, connectedThrough, relayHost } from './relay-copy.ts'

test('official clouds stay Cloudflare and any other host is named', () => {
  assert.equal(relayHost('https://api.codync.dev'), null)
  assert.equal(relayHost('https://dev-api.codync.dev/'), null)
  assert.equal(relayHost(null), null)
  assert.equal(relayHost('https://relay.example.com'), 'relay.example.com')
  assert.equal(relayHost('http://127.0.0.1:8788'), '127.0.0.1:8788')
  assert.equal(relayHost('https://[2001:db8::1]:8443'), '[2001:db8::1]:8443')
  assert.equal(relayHost('https://[2001:db8::1]'), '[2001:db8::1]')
  assert.equal(connectedThrough('https://api.codync.dev'), 'Connected through Cloudflare')
  assert.equal(connectedThrough('https://relay.example.com'), 'Connected through Private relay (relay.example.com)')
  assert.equal(
    cloudStatusLine({ enabled: true, url: 'https://relay.example.com', connected: true }),
    'Reachable from anywhere through Private relay (relay.example.com) (encrypted).',
  )
  assert.equal(
    cloudStatusLine({ enabled: false, url: 'https://relay.example.com' }),
    'Off: only Wi-Fi and Tailscale reach it, nothing goes through Cloudflare.',
  )
})
