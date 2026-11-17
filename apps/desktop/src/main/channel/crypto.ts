import { createHash, createPrivateKey, createPublicKey, diffieHellman, generateKeyPairSync, hkdfSync, randomBytes, sign, verify, type KeyObject } from 'node:crypto'
import { chacha20poly1305 } from '@noble/ciphers/chacha.js'

// The device side of the relay protocol's crypto (docs/reference/remote-relay.md §6), on node:crypto, except
// ChaCha20-Poly1305: Electron's BoringSSL-based node:crypto has no such cipher, so frames use @noble/ciphers.
// Checked against docs/reference/fixtures/remote-relay-vectors.json, like the iPhone's `RelayCrypto`.

export const b64url = (b: Uint8Array) => Buffer.from(b).toString('base64url')
export const unb64url = (s: string) => Buffer.from(s, 'base64url')
const text = (s: string) => Buffer.from(s, 'utf8')

const u64be = (n: number) => {
  const out = Buffer.alloc(8)
  out.writeBigUInt64BE(BigInt(n))
  return out
}

/** A raw 32-byte key as a KeyObject: PKCS#8 for a private key (fixed prefix + key), JWK for a public one. */
const PKCS8 = { ed25519: '302e020100300506032b657004220420', x25519: '302e020100300506032b656e04220420' }
export const privateKey = (kind: keyof typeof PKCS8, raw: Uint8Array) =>
  createPrivateKey({ key: Buffer.concat([Buffer.from(PKCS8[kind], 'hex'), raw]), format: 'der', type: 'pkcs8' })
const publicKey = (crv: 'Ed25519' | 'X25519', raw: Uint8Array) => createPublicKey({ key: { kty: 'OKP', crv, x: b64url(raw) }, format: 'jwk' })
export const rawPublic = (key: KeyObject) => unb64url((createPublicKey(key).export({ format: 'jwk' }) as { x: string }).x)

export const computerIdRaw = (hostSignPub: Uint8Array) => createHash('sha256').update(hostSignPub).digest().subarray(0, 16)

/** The device's account key: an Ed25519 private key and its raw public key. */
export interface DeviceKey {
  key: KeyObject
  pub: Buffer
}

export interface ChannelKeys {
  d2h: Buffer
  h2d: Buffer
}

/** §6.2 device side: `hello` to send, then `finish(welcome)` checks the computer's signature and derives the keys. */
export function deviceHello(device: DeviceKey, hostSignPub: Uint8Array, eph: KeyObject = generateKeyPairSync('x25519').privateKey, n: Buffer = randomBytes(32)) {
  const cid = computerIdRaw(hostSignPub)
  const ekD = rawPublic(eph)
  const sig = sign(null, Buffer.concat([text('codync/hs1/v1'), cid, device.pub, ekD, n]), device.key)
  return {
    hello: { t: 'hello', v: 1, dk: b64url(device.pub), ek: b64url(ekD), n: b64url(n), sig: b64url(sig) },
    finish(welcome: { ek: string; sig: string }): ChannelKeys {
      const ekH = unb64url(welcome.ek)
      const th = createHash('sha256').update(Buffer.concat([text('codync/hs2/v1'), cid, device.pub, ekD, n, ekH])).digest()
      if (!verify(null, th, publicKey('Ed25519', hostSignPub), unb64url(welcome.sig))) throw new Error("The computer's signature doesn't match.")
      const ss = diffieHellman({ privateKey: eph, publicKey: publicKey('X25519', ekH) })
      if (ss.every((b) => b === 0)) throw new Error('Bad key exchange.')
      const key = (info: string) => Buffer.from(hkdfSync('sha256', ss, th, info, 32))
      return { d2h: key('codync/d2h/v1'), h2d: key('codync/h2d/v1') }
    },
  }
}

/** §4.2 B: what the device commits to before the computer shows its half of the code. */
export const sasCommit = (dk: Uint8Array, nD: Uint8Array) => createHash('sha256').update(Buffer.concat([text('codync/sascommit/v1'), dk, nD])).digest()

// ---- frames (§6.3) ----

export const CHUNK = 256 * 1024
const MAX_MESSAGE = 16 * 1024 * 1024
const nonce = (c: number) => Buffer.concat([Buffer.alloc(4), u64be(c)])
const aad = (c: number) => Buffer.concat([text('codync/frame/v1'), u64be(c)])

export function sealFrame(key: Buffer, c: number, final: boolean, chunk: Uint8Array): string {
  return b64url(chacha20poly1305(key, nonce(c), aad(c)).encrypt(Buffer.concat([Buffer.from([final ? 0 : 1]), chunk])))
}

export function openFrame(key: Buffer, c: number, d: string): { final: boolean; chunk: Buffer } {
  const plain = Buffer.from(chacha20poly1305(key, nonce(c), aad(c)).decrypt(unb64url(d)))
  if (plain[0] !== 0 && plain[0] !== 1) throw new Error('Bad frame flag.')
  return { final: plain[0] === 0, chunk: plain.subarray(1) }
}

/** Both directions of one channel: counters, 256 KiB chunks and reassembly. */
export class FrameCodec {
  private sendC = 0
  private recvC = 0
  private parts: Buffer[] = []
  private size = 0
  private readonly keys: ChannelKeys

  constructor(keys: ChannelKeys) {
    this.keys = keys
  }

  seal(message: unknown): { t: 'f'; c: number; d: string }[] {
    const bytes = text(JSON.stringify(message))
    const out: { t: 'f'; c: number; d: string }[] = []
    for (let at = 0; at === 0 || at < bytes.length; at += CHUNK) {
      out.push({ t: 'f', c: this.sendC, d: sealFrame(this.keys.d2h, this.sendC, at + CHUNK >= bytes.length, bytes.subarray(at, at + CHUNK)) })
      this.sendC += 1
    }
    return out
  }

  /** The message once its last chunk arrives; throws on a counter gap, a bad tag or a message past 16 MiB. */
  open(frame: { c: number; d: string }): unknown {
    if (frame.c !== this.recvC) throw new Error(`Frame ${frame.c}, expected ${this.recvC}.`)
    const { final, chunk } = openFrame(this.keys.h2d, frame.c, frame.d)
    this.recvC += 1
    this.size += chunk.length
    if (this.size > MAX_MESSAGE) throw new Error('Message too large.')
    this.parts.push(chunk)
    if (!final) return undefined
    const message = JSON.parse(Buffer.concat(this.parts).toString('utf8')) as unknown
    this.parts = []
    this.size = 0
    return message
  }
}
