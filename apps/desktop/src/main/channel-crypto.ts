import { createCipheriv, createDecipheriv, createHash, createPrivateKey, createPublicKey, diffieHellman, hkdfSync, randomBytes, sign, verify, type KeyObject } from 'node:crypto'

/**
 * Protocol-v1 primitives of the end-to-end channel between a device and a host (host `remote::crypto`,
 * the phones' `RelayCrypto`): the handshake, its frames and the access-request code. Checked against
 * the shared vectors in `docs/reference/fixtures/remote-relay-vectors.json`.
 */

const CHUNK_SIZE = 256 * 1024
const MAX_OUTBOUND = 1024 * 1024
const MAX_INBOUND = 16 * 1024 * 1024
const MAX_COUNTER = 2 ** 32

// DER prefixes that wrap a raw 32-byte key: PKCS#8 private and SPKI public, Ed25519 and X25519.
const ED25519_PRIVATE = Buffer.from('302e020100300506032b657004220420', 'hex')
const ED25519_PUBLIC = Buffer.from('302a300506032b6570032100', 'hex')
const X25519_PRIVATE = Buffer.from('302e020100300506032b656e04220420', 'hex')
const X25519_PUBLIC = Buffer.from('302a300506032b656e032100', 'hex')

const label = (text: string) => Buffer.from(text, 'utf8')
const sha256 = (...parts: Buffer[]) => parts.reduce((h, p) => h.update(p), createHash('sha256')).digest()
const unb64 = (text: string, size: number) => {
  const bytes = Buffer.from(text, 'base64url')
  if (bytes.length !== size) throw new Error('Malformed key')
  return bytes
}
/** The raw 32 bytes of a public key: the end of its SPKI encoding. */
const rawPublic = (key: KeyObject) => createPublicKey(key).export({ format: 'der', type: 'spki' }).subarray(-32)

export interface DeviceKey {
  key: KeyObject
  /** Raw Ed25519 public key, b64url. */
  publicKey: string
}

/** The device key of a 32-byte Ed25519 seed. */
export function signingKey(seed: Buffer): DeviceKey {
  const key = createPrivateKey({ key: Buffer.concat([ED25519_PRIVATE, seed]), format: 'der', type: 'pkcs8' })
  return { key, publicKey: rawPublic(key).toString('base64url') }
}

/** The id a computer's Ed25519 key stands for: the first 16 bytes of its hash. */
export function computerId(signKey: string) {
  return sha256(unb64(signKey, 32)).subarray(0, 16).toString('base64url')
}

export interface ChannelKeys {
  d2h: Buffer
  h2d: Buffer
}

/** One handshake attempt: `hello` to send, then `finish` with the host's `welcome`. */
export class Handshake {
  private readonly hostKey: string
  private readonly device: DeviceKey
  private readonly nonce: Buffer
  private readonly cid: Buffer
  private readonly dk: Buffer
  private readonly ephemeral: KeyObject
  private readonly ek: Buffer

  constructor(hostKey: string, device: DeviceKey, ephemeralSeed = randomBytes(32), nonce = randomBytes(32)) {
    this.hostKey = hostKey
    this.device = device
    this.nonce = nonce
    this.cid = sha256(unb64(hostKey, 32)).subarray(0, 16)
    this.dk = unb64(device.publicKey, 32)
    this.ephemeral = createPrivateKey({ key: Buffer.concat([X25519_PRIVATE, ephemeralSeed]), format: 'der', type: 'pkcs8' })
    this.ek = rawPublic(this.ephemeral)
  }

  hello() {
    const input = Buffer.concat([label('codync/hs1/v1'), this.cid, this.dk, this.ek, this.nonce])
    return {
      t: 'hello',
      v: 1,
      dk: this.device.publicKey,
      ek: this.ek.toString('base64url'),
      n: this.nonce.toString('base64url'),
      sig: sign(null, input, this.device.key).toString('base64url'),
    }
  }

  /** Checks the host's signature over the transcript and derives the keys of both directions. */
  finish(ek: string, sig: string): ChannelKeys {
    const ekH = unb64(ek, 32)
    const transcript = sha256(label('codync/hs2/v1'), this.cid, this.dk, this.ek, this.nonce, ekH)
    const hostKey = createPublicKey({ key: Buffer.concat([ED25519_PUBLIC, unb64(this.hostKey, 32)]), format: 'der', type: 'spki' })
    if (!verify(null, transcript, hostKey, unb64(sig, 64))) throw new Error("This computer's identity changed. Remove it from your account and add it again.")
    const peer = createPublicKey({ key: Buffer.concat([X25519_PUBLIC, ekH]), format: 'der', type: 'spki' })
    const shared = diffieHellman({ privateKey: this.ephemeral, publicKey: peer })
    if (shared.every((b) => b === 0)) throw new Error('Invalid zero shared secret')
    const derive = (info: string) => Buffer.from(hkdfSync('sha256', shared, transcript, info, 32))
    return { d2h: derive('codync/d2h/v1'), h2d: derive('codync/h2d/v1') }
  }
}

/** Frame `counter`'s nonce (4 zero bytes, then the counter) and associated data. */
function frameParams(counter: number) {
  const be = Buffer.alloc(8)
  be.writeBigUInt64BE(BigInt(counter))
  return { nonce: Buffer.concat([Buffer.alloc(4), be]), aad: Buffer.concat([label('codync/frame/v1'), be]) }
}

export function sealFrame(key: Buffer, counter: number, last: boolean, chunk: Buffer) {
  const { nonce, aad } = frameParams(counter)
  const cipher = createCipheriv('chacha20-poly1305', key, nonce, { authTagLength: 16 })
  cipher.setAAD(aad, { plaintextLength: chunk.length + 1 })
  return Buffer.concat([cipher.update(Buffer.from([last ? 0 : 1])), cipher.update(chunk), cipher.final(), cipher.getAuthTag()])
}

export function openFrame(key: Buffer, counter: number, data: Buffer): { last: boolean; chunk: Buffer } {
  if (data.length < 17) throw new Error('Malformed encrypted frame')
  const { nonce, aad } = frameParams(counter)
  const decipher = createDecipheriv('chacha20-poly1305', key, nonce, { authTagLength: 16 })
  decipher.setAAD(aad, { plaintextLength: data.length - 16 })
  decipher.setAuthTag(data.subarray(-16))
  const plain = Buffer.concat([decipher.update(data.subarray(0, -16)), decipher.final()])
  if (plain[0] !== 0 && plain[0] !== 1) throw new Error('Invalid chunk flag')
  return { last: plain[0] === 0, chunk: plain.subarray(1) }
}

/** The sending half of a direction: one message becomes frames numbered from 0. */
export class FrameSealer {
  private counter = 0
  private readonly key: Buffer
  constructor(key: Buffer) {
    this.key = key
  }

  seal(message: Buffer): { c: number; d: string }[] {
    if (message.length > MAX_OUTBOUND) throw new Error('Message is too large')
    const count = Math.max(1, Math.ceil(message.length / CHUNK_SIZE))
    if (this.counter + count > MAX_COUNTER) throw new Error('Channel must be rekeyed')
    return Array.from({ length: count }, (_, i) => {
      const c = this.counter++
      return { c, d: sealFrame(this.key, c, i === count - 1, message.subarray(i * CHUNK_SIZE, (i + 1) * CHUNK_SIZE)).toString('base64url') }
    })
  }
}

/** The receiving half: frames in order, a whole message once the last one arrives. */
export class FrameOpener {
  private counter = 0
  private parts: Buffer[] = []
  private size = 0
  private readonly key: Buffer
  constructor(key: Buffer) {
    this.key = key
  }

  open(c: number, d: string): Buffer | null {
    if (c !== this.counter) throw new Error('Encrypted frame is out of order')
    const { last, chunk } = openFrame(this.key, c, Buffer.from(d, 'base64url'))
    this.counter++
    this.size += chunk.length
    if (this.size > MAX_INBOUND) throw new Error('Message is too large')
    this.parts.push(chunk)
    if (!last) return null
    const message = Buffer.concat(this.parts)
    this.parts = []
    this.size = 0
    return message
  }
}

/** What an access request commits to before the host answers with its own nonce. */
export function sasCommit(deviceKey: string, deviceNonce: Buffer) {
  return sha256(label('codync/sascommit/v1'), unb64(deviceKey, 32), deviceNonce).toString('base64url')
}

/** The 6-digit code both sides show for an access request. */
export function sasCode(hostKey: string, deviceKey: string, deviceNonce: Buffer, hostNonce: string) {
  const hash = sha256(label('codync/sas/v2'), unb64(hostKey, 32), unb64(deviceKey, 32), deviceNonce, unb64(hostNonce, 32))
  return String(hash.readUInt32BE(0) % 1_000_000).padStart(6, '0')
}
