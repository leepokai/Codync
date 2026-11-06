/** Codync-Sig (§5). The signed authority is the public Host header, not the socket's own URL. */

import { b64url, fromB64url, sha256, utf8, verifyEd25519 } from './codec.ts'

export const SIG_WINDOW_MS = 300_000
export const NONCE_TTL_MS = 600_000

export interface SignedRequest {
  kid: string
  ts: number
  nonce: string
}

/**
 * The authority computers sign for this relay: the host of its public URL, with the port only when
 * it isn't the scheme's default, as the host derives it from the same address.
 * `https://Relay.Example.com:443` → `relay.example.com`, `http://127.0.0.1:443` → `127.0.0.1:443`.
 */
export function publicAuthority(publicUrl: string): string {
  const url = new URL(publicUrl)
  if (!['http:', 'https:'].includes(url.protocol) || url.pathname !== '/' || url.search || url.hash || url.username || url.password) {
    throw new Error('CODYNC_PRIVATE_RELAY_PUBLIC_URL must be an http(s) address with no path, such as https://relay.example.com.')
  }
  return url.host.toLowerCase()
}

export async function sigCanonical(
  method: string,
  authority: string,
  pathAndQuery: string,
  ts: number,
  nonce: string,
  body: Uint8Array,
): Promise<string> {
  return ['codync-sig-v1', method.toUpperCase(), authority.toLowerCase(), pathAndQuery, String(ts), nonce, b64url(await sha256(body))].join('\n')
}

/**
 * Checks the header's signature and time window. Replay of `(kid, nonce)` is the caller's job.
 * `authority` is this relay's `publicAuthority`.
 */
export async function verifyCodyncSig(
  header: string | undefined,
  method: string,
  authority: string,
  pathAndQuery: string,
  body: Uint8Array,
  now: number,
): Promise<SignedRequest | null> {
  if (!header) return null
  const fields = new Map<string, string>()
  for (const part of header.split(',')) {
    const i = part.indexOf('=')
    if (i <= 0) return null
    fields.set(part.slice(0, i).trim(), part.slice(i + 1).trim())
  }
  const kid = fields.get('kid')
  const pub = fromB64url(kid, 32)
  const sig = fromB64url(fields.get('sig'), 64)
  const nonce = fields.get('nonce') ?? ''
  const tsText = fields.get('ts') ?? ''
  if (fields.get('v') !== '1' || !pub || !sig || !kid || !fromB64url(nonce, 16) || !/^\d{1,16}$/.test(tsText)) return null
  const ts = Number(tsText)
  if (Math.abs(now - ts) > SIG_WINDOW_MS) return null
  const canonical = await sigCanonical(method, authority, pathAndQuery, ts, nonce, body)
  if (!(await verifyEd25519(pub, sig, utf8(canonical)))) return null
  return { kid, ts, nonce }
}
