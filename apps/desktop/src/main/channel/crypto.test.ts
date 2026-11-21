import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { b64url, deviceHello, FrameCodec, openFrame, privateKey, rawPublic, sasCode, sasCommit, sealFrame, unb64url } from './crypto.ts'

const vectors = JSON.parse(readFileSync(new URL('../../../../../docs/reference/fixtures/remote-relay-vectors.json', import.meta.url), 'utf8'))
const { keys, handshake: hs, frames, sas } = vectors
const device = { key: privateKey('ed25519', unb64url(keys.deviceSignSeed)), pub: unb64url(keys.deviceSignPub) }
const channelKeys = { d2h: unb64url(hs.kD2H), h2d: unb64url(hs.kH2D) }

test('the hello and the channel keys match the protocol vectors', () => {
  assert.equal(b64url(rawPublic(device.key)), keys.deviceSignPub)
  const d = deviceHello(device, unb64url(keys.hostSignPub), privateKey('x25519', unb64url(hs.deviceEphPriv)), unb64url(hs.n))
  assert.equal(d.hello.ek, hs.deviceEphPub)
  assert.equal(d.hello.sig, hs.hs1Sig)
  const derived = d.finish({ ek: hs.hostEphPub, sig: hs.hs2Sig })
  assert.equal(b64url(derived.d2h), hs.kD2H)
  assert.equal(b64url(derived.h2d), hs.kH2D)
})

test('a welcome another key signed is refused', () => {
  const d = deviceHello(device, unb64url(keys.deviceSignPub), privateKey('x25519', unb64url(hs.deviceEphPriv)), unb64url(hs.n))
  assert.throws(() => d.finish({ ek: hs.hostEphPub, sig: hs.hs2Sig }))
})

test('frames match the protocol vectors', () => {
  const codec = new FrameCodec(channelKeys)
  const [first] = codec.seal(JSON.parse(frames[0].message))
  assert.equal(first?.d, frames[0].d)
  assert.deepEqual(codec.open({ c: 0, d: frames[1].d }), JSON.parse(frames[1].message))
  assert.equal(sealFrame(channelKeys.d2h, 1, false, Buffer.from(frames[2].message)), frames[2].d)
  assert.equal(openFrame(channelKeys.d2h, 1, frames[2].d).final, false)
})

test('a frame out of order or tampered with is refused', () => {
  assert.throws(() => new FrameCodec(channelKeys).open({ c: 1, d: frames[1].d }))
  const bad = unb64url(frames[1].d)
  bad[0] = (bad[0] ?? 0) ^ 1
  assert.throws(() => new FrameCodec(channelKeys).open({ c: 0, d: b64url(bad) }))
})

test('a long message goes in 256 KiB chunks and comes back whole', () => {
  const long = { text: 'x'.repeat(600 * 1024) }
  const sealed = new FrameCodec(channelKeys).seal(long)
  assert.equal(sealed.length, 3)
  // The host's side of the same channel opens with the device's sending key.
  const host = new FrameCodec({ d2h: channelKeys.h2d, h2d: channelKeys.d2h })
  const opened = sealed.map((f) => host.open(f))
  assert.deepEqual(opened.slice(0, 2), [undefined, undefined])
  assert.deepEqual(opened[2], long)
})

test('the access request commit and code match the protocol vectors', () => {
  assert.equal(b64url(sasCommit(device.pub, unb64url(sas.deviceNonce))), sas.commit)
  assert.equal(sasCode(unb64url(keys.hostSignPub), device.pub, unb64url(sas.deviceNonce), unb64url(sas.hostNonce)), sas.code)
})
