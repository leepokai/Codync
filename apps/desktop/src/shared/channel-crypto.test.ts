import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { sign } from 'node:crypto'
import { test } from 'node:test'
import { computerId, FrameOpener, FrameSealer, Handshake, openFrame, sasCode, sasCommit, sealFrame, signingKey } from '../main/channel-crypto.ts'

const v = JSON.parse(readFileSync(new URL('../../../../docs/reference/fixtures/remote-relay-vectors.json', import.meta.url), 'utf8'))
const raw = (text: string) => Buffer.from(text, 'base64url')
const device = signingKey(raw(v.keys.deviceSignSeed))
const host = signingKey(raw(v.keys.hostSignSeed))
const h = v.handshake

test('keys and the computer id match the host', () => {
  assert.equal(device.publicKey, v.keys.deviceSignPub)
  assert.equal(host.publicKey, v.keys.hostSignPub)
  assert.equal(computerId(v.keys.hostSignPub), v.keys.computerId)
})

test('the handshake signs the host vector and derives its channel keys', () => {
  const handshake = new Handshake(v.keys.hostSignPub, device, raw(h.deviceEphPriv), raw(h.n))
  const hello = handshake.hello()
  assert.deepEqual([hello.dk, hello.ek, hello.n, hello.sig], [v.keys.deviceSignPub, h.deviceEphPub, h.n, h.hs1Sig])
  const keys = handshake.finish(h.hostEphPub, h.hs2Sig)
  assert.equal(keys.d2h.toString('base64url'), h.kD2H)
  assert.equal(keys.h2d.toString('base64url'), h.kH2D)
})

test('a welcome not signed by the pinned computer is refused', () => {
  const handshake = new Handshake(v.keys.hostSignPub, device, raw(h.deviceEphPriv), raw(h.n))
  const forged = sign(null, raw(h.transcriptHash), device.key).toString('base64url')
  assert.throws(() => handshake.finish(h.hostEphPub, forged), /identity changed/)
})

test('frames seal and open as the host does, bound to their counter', () => {
  for (const f of v.frames) {
    const key = raw(f.dir === 'd2h' ? h.kD2H : h.kH2D)
    const sealed = sealFrame(key, f.c, f.final, Buffer.from(f.message))
    assert.equal(sealed.toString('base64url'), f.d)
    assert.deepEqual(openFrame(key, f.c, sealed), { last: f.final, chunk: Buffer.from(f.message) })
    assert.throws(() => openFrame(key, f.c + 1, sealed))
  }
})

test('a message larger than a chunk arrives whole, and only in order', () => {
  const key = Buffer.alloc(32, 7)
  const sealer = new FrameSealer(key)
  const opener = new FrameOpener(key)
  const big = Buffer.alloc(256 * 1024 * 2 + 5, 'x')
  const frames = sealer.seal(big)
  assert.equal(frames.length, 3)
  assert.equal(opener.open(frames[0].c, frames[0].d), null)
  assert.equal(opener.open(frames[1].c, frames[1].d), null)
  assert.deepEqual(opener.open(frames[2].c, frames[2].d), big)
  sealer.seal(Buffer.from('a'))
  const b = sealer.seal(Buffer.from('b'))[0]
  assert.throws(() => opener.open(b.c, b.d), /out of order/)
  assert.throws(() => sealer.seal(Buffer.alloc(1024 * 1024 + 1)), /too large/)
})

test('the access request commits to its nonce and shows the host code', () => {
  const nonce = raw(v.sas.deviceNonce)
  assert.equal(sasCommit(v.keys.deviceSignPub, nonce), v.sas.commit)
  assert.equal(sasCode(v.keys.hostSignPub, v.keys.deviceSignPub, nonce, v.sas.hostNonce), v.sas.code)
})
