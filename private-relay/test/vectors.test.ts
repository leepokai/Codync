import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { computerIdFor, fromB64url, verifyEd25519 } from '../src/codec.ts'
import { parseAcl } from '../src/acl.ts'
import { publicAuthority, verifyCodyncSig } from '../src/sig.ts'
import { RelayStore } from '../src/store.ts'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'

const vectors = JSON.parse(readFileSync(new URL('../../docs/reference/fixtures/remote-relay-vectors.json', import.meta.url), 'utf8')) as {
  keys: { hostSignPub: string; deviceSignPub: string; computerId: string }
  requestSig: { header: string; ts: number; authority: string }
  acl: { d: string; sig: string; json: string }
}

test('the vector computer id, request signature and ACL bytes verify', async () => {
  const pub = fromB64url(vectors.keys.hostSignPub, 32)!
  assert.equal(await computerIdFor(pub), vectors.keys.computerId)
  const header = vectors.requestSig.header.replace(/^Codync-Sig:\s*/, '')
  const signed = await verifyCodyncSig(
    header,
    'GET',
    vectors.requestSig.authority,
    '/v1/relay/device/NHUPmL1Z_PyUbaRaqr6TOw?v=1',
    new Uint8Array(),
    vectors.requestSig.ts,
  )
  assert.equal(signed?.kid, vectors.keys.deviceSignPub)
  assert.equal(await verifyCodyncSig(header, 'GET', vectors.requestSig.authority, '/v1/relay/device/NHUPmL1Z_PyUbaRaqr6TOw?v=1', new Uint8Array(), vectors.requestSig.ts + 300_001), null)

  const dir = mkdtempSync(join(tmpdir(), 'codync-relay-nonce-'))
  const store = new RelayStore(join(dir, 'relay.sqlite'))
  assert.equal(store.rememberNonce(signed!.kid, signed!.nonce, vectors.requestSig.ts), true)
  assert.equal(store.rememberNonce(signed!.kid, signed!.nonce, vectors.requestSig.ts), false)
  store.close()

  const d = fromB64url(vectors.acl.d)!
  const sig = fromB64url(vectors.acl.sig, 64)!
  assert.equal(await verifyEd25519(pub, sig, d), true)
  const acl = parseAcl(d)
  assert.equal(acl?.computerId, vectors.keys.computerId)
  assert.equal(acl?.ver, 1)
  assert.equal(acl?.devices[0]?.dk, vectors.keys.deviceSignPub)
})

test('the signed authority keeps a real port and drops the defaults', () => {
  assert.equal(publicAuthority('https://Example.COM:8443'), 'example.com:8443')
  assert.equal(publicAuthority('https://relay.example.com:443/'), 'relay.example.com')
  assert.equal(publicAuthority('http://127.0.0.1:80'), '127.0.0.1')
  assert.equal(publicAuthority('http://127.0.0.1:443'), '127.0.0.1:443', 'only the scheme default drops')
  assert.equal(publicAuthority('https://relay.example.com:80'), 'relay.example.com:80')
  assert.equal(publicAuthority('http://[::1]:8788'), '[::1]:8788')
  assert.throws(() => publicAuthority('https://relay.example.com/path'))
})
