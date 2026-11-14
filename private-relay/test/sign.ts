/** Ed25519 request signing for tests. Node has no seed import, so the PKCS#8 prefix wraps the 32-byte seed. */

import { createPrivateKey, createPublicKey, sign, type KeyObject } from 'node:crypto'
import { b64url, fromB64url, sha256 } from '../src/codec.ts'
import { sigCanonical } from '../src/sig.ts'

const PKCS8_PREFIX = Buffer.from('302e020100300506032b657004220420', 'hex')

function privateKeyFromSeed(seed: Uint8Array): KeyObject {
  return createPrivateKey({ key: Buffer.concat([PKCS8_PREFIX, Buffer.from(seed)]), format: 'der', type: 'pkcs8' })
}

export function signBytes(seedB64: string, message: Uint8Array): string {
  const seed = fromB64url(seedB64, 32)
  if (!seed) throw new Error('bad seed')
  return b64url(sign(null, message, privateKeyFromSeed(seed)))
}

/** A new Ed25519 key as base64url seed and public key. */
export function freshKey(): { seed: string; pub: string } {
  const seed = crypto.getRandomValues(new Uint8Array(32))
  const der = createPublicKey(privateKeyFromSeed(seed)).export({ format: 'der', type: 'spki' })
  return { seed: b64url(seed), pub: b64url(der.subarray(der.length - 32)) }
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
