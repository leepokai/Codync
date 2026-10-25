/** Base64url and the computer id. Same rules as `cloud/src/auth.ts`. */

export const b64url = (bytes: Uint8Array): string => {
  let s = ''
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000))
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')
}

/** Strict base64url (no padding). `null` when malformed or not `len` bytes. */
export function fromB64url(s: unknown, len?: number): Uint8Array | null {
  if (typeof s !== 'string' || !/^[A-Za-z0-9_-]*$/.test(s) || s.length % 4 === 1) return null
  const pad = s.length % 4 === 0 ? '' : '='.repeat(4 - (s.length % 4))
  let bytes: Uint8Array
  try {
    bytes = Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/') + pad), (c) => c.charCodeAt(0))
  } catch {
    return null
  }
  if (len !== undefined && bytes.length !== len) return null
  return b64url(bytes) === s ? bytes : null
}

export const utf8 = (s: string): Uint8Array => new TextEncoder().encode(s)

export const sha256 = async (bytes: Uint8Array): Promise<Uint8Array> =>
  new Uint8Array(await crypto.subtle.digest('SHA-256', bytes))

export const randomId = (n = 16): string => b64url(crypto.getRandomValues(new Uint8Array(n)))

/** `computerId = b64url(SHA-256(hostSignPub)[0..16])`. */
export const computerIdFor = async (signPub: Uint8Array): Promise<string> => b64url((await sha256(signPub)).subarray(0, 16))

export const COMPUTER_ID = /^[A-Za-z0-9_-]{22}$/

export async function verifyEd25519(pub: Uint8Array, sig: Uint8Array, msg: Uint8Array): Promise<boolean> {
  if (pub.length !== 32 || sig.length !== 64) return false
  try {
    const key = await crypto.subtle.importKey('raw', pub, { name: 'Ed25519' }, false, ['verify'])
    return await crypto.subtle.verify({ name: 'Ed25519' }, key, sig, msg)
  } catch {
    return false
  }
}
