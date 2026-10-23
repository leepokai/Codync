/** Ed25519 request signing for tests. Node has no seed import, so the PKCS#8 prefix wraps the 32-byte seed. */

import { createPrivateKey, sign } from 'node:crypto'
import { b64url, fromB64url, sha256 } from '../src/codec.ts'
import { sigCanonical } from '../src/sig.ts'

const PKCS8_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex')

export function signBytes(seedB64: string, message: Uint8Array): string {
  const seed = fromB64url(seedB64, 32)
  if (!seed) throw new Error('bad seed')
  const key = createPrivateKey({ key: Buffer.concat([PKCS8_PREFIX, Buffer.from(seed)]), format: 'der', type: 'pkcs8' })
  return b64url(sign(null, message, key))
}

export async function sigHeader(
  seedB64: string,
  kid: string,
  method: string,
  authority: string,
  pathAndQuery: string,
  body: Uint8Array,
  ts = Date.now(),
): Promise<string> {
  const nonce = b64url(crypto.getRandomValues(new Uint8Array(16)))
  const canonical = await sigCanonical(method, authority, pathAndQuery, ts, nonce, body)
  const sig = signBytes(seedB64, new TextEncoder().encode(canonical))
  return `v=1,kid=${kid},ts=${ts},nonce=${nonce},sig=${sig}`
}

export const emptySha = async (): Promise<string> => b64url(await sha256(new Uint8Array()))
