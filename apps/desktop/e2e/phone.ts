import { createCipheriv, createDecipheriv, createHash, createPublicKey, diffieHellman, generateKeyPairSync, hkdfSync, randomBytes, sign, type KeyObject } from 'node:crypto'

// A phone with no account on the encrypted channel (docs/reference/remote-relay.md §6), for what only a phone can make
// the host do: asking to pair.

const b64 = (bytes: Uint8Array) => Buffer.from(bytes).toString('base64url')
const unb64 = (text: string) => Buffer.from(text, 'base64url')
const rawKey = (key: KeyObject) => unb64(key.export({ format: 'jwk' }).x as string)
const sha256 = (...parts: (string | Uint8Array)[]) => parts.reduce((hash, part) => hash.update(part), createHash('sha256')).digest()

const FINAL = Buffer.from([0x00])
const TAG_LENGTH = 16

function frameParams(counter: number) {
  const be = Buffer.alloc(8)
  be.writeBigUInt64BE(BigInt(counter))
  return { nonce: Buffer.concat([Buffer.alloc(4), be]), aad: Buffer.concat([Buffer.from('codync/frame/v1'), be]) }
}

function sealFrame(key: Uint8Array, counter: number, message: string) {
  const { nonce, aad } = frameParams(counter)
  const plain = Buffer.concat([FINAL, Buffer.from(message)])
  const cipher = createCipheriv('chacha20-poly1305', key, nonce, { authTagLength: TAG_LENGTH })
  cipher.setAAD(aad, { plaintextLength: plain.length })
  return Buffer.concat([cipher.update(plain), cipher.final(), cipher.getAuthTag()])
}

function openFrame(key: Uint8Array, counter: number, sealed: Buffer) {
  const { nonce, aad } = frameParams(counter)
  const text = sealed.subarray(0, sealed.length - TAG_LENGTH)
  const decipher = createDecipheriv('chacha20-poly1305', key, nonce, { authTagLength: TAG_LENGTH })
  decipher.setAAD(aad, { plaintextLength: text.length })
  decipher.setAuthTag(sealed.subarray(sealed.length - TAG_LENGTH))
  return Buffer.concat([decipher.update(text), decipher.final()]).subarray(1).toString()
}

/** Sends `pair` for a pairing code and resolves with the host's reply: `ok`, or `err` with its status and message. */
export async function pairAsSignedOutPhone(port: number, computerId: string, code: string, name: string) {
  const device = generateKeyPairSync('ed25519')
  const ephemeral = generateKeyPairSync('x25519')
  const cid = unb64(computerId)
  const [dk, ek, n] = [rawKey(device.publicKey), rawKey(ephemeral.publicKey), randomBytes(32)]
  const socket = new WebSocket(`ws://127.0.0.1:${port}/channel?v=1`)
  return new Promise<{ ok?: unknown; err?: { status: number; message: string } }>((resolve, reject) => {
    let receiveKey: Buffer
    socket.onerror = () => reject(new Error('The sandbox host refused the channel.'))
    socket.onopen = () => socket.send(JSON.stringify({
      t: 'hello', v: 1, dk: b64(dk), ek: b64(ek), n: b64(n), pair: true,
      sig: b64(sign(null, Buffer.concat([Buffer.from('codync/hs1/v1'), cid, dk, ek, n]), device.privateKey)),
    }))
    socket.onmessage = ({ data }) => {
      const message = JSON.parse(String(data)) as { t: string; ek?: string; c?: number; d?: string }
      if (message.t === 'welcome') {
        const hostEk = unb64(message.ek as string)
        const th = sha256('codync/hs2/v1', cid, dk, ek, n, hostEk)
        const shared = diffieHellman({ privateKey: ephemeral.privateKey, publicKey: x25519PublicKey(hostEk) })
        const key = (info: string) => Buffer.from(hkdfSync('sha256', shared, th, info, 32))
        receiveKey = key('codync/h2d/v1')
        const body = { id: 1, m: 'pair', b: { code, name, platform: 'android' } }
        socket.send(JSON.stringify({ t: 'f', c: 0, d: b64(sealFrame(key('codync/d2h/v1'), 0, JSON.stringify(body))) }))
      } else if (message.t === 'f') {
        resolve(JSON.parse(openFrame(receiveKey, message.c as number, unb64(message.d as string))))
        socket.close()
      } else if (message.t === 'reject') {
        reject(new Error(`The sandbox host rejected the phone: ${String(data)}`))
      }
    }
  })
}

function x25519PublicKey(raw: Uint8Array) {
  return createPublicKey({ key: { kty: 'OKP', crv: 'X25519', x: b64(raw) }, format: 'jwk' })
}
