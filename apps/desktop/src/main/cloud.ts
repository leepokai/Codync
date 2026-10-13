import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, randomBytes, sign, type KeyObject } from 'node:crypto'
import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { app, ipcMain, safeStorage } from 'electron'
import type { AccountService } from './account'

/**
 * The Codync cloud's user-facing `/v1` API (§8.4) with this install's device key (§3.2): an
 * Ed25519 key per account, kept encrypted with the OS keychain, that signs `Codync-Sig` (§5).
 */

const b64url = (b: Buffer) => b.toString('base64url')

interface Identity {
  key: KeyObject
  publicKey: string
}

const identities = new Map<string, Identity>()

async function identity(userId: string): Promise<Identity> {
  const cached = identities.get(userId)
  if (cached) return cached
  const path = join(app.getPath('userData'), `device-key-${createHash('sha256').update(userId).digest('hex').slice(0, 16)}.bin`)
  let seed: Buffer | null = null
  try {
    const sealed = await fs.readFile(path)
    seed = Buffer.from(safeStorage.isEncryptionAvailable() ? safeStorage.decryptString(sealed) : sealed.toString('utf8'), 'base64url')
  } catch {}
  let key: KeyObject
  if (seed?.length === 32) {
    key = createPrivateKey({ key: { kty: 'OKP', crv: 'Ed25519', d: b64url(seed), x: publicFromSeed(seed) }, format: 'jwk' })
  } else {
    key = generateKeyPairSync('ed25519').privateKey
    const d = (key.export({ format: 'jwk' }) as { d: string }).d
    const raw = safeStorage.isEncryptionAvailable() ? safeStorage.encryptString(d) : Buffer.from(d)
    await fs.mkdir(app.getPath('userData'), { recursive: true })
    await fs.writeFile(path, raw, { mode: 0o600 })
  }
  const publicKey = (key.export({ format: 'jwk' }) as { x: string }).x
  const made = { key, publicKey }
  identities.set(userId, made)
  return made
}

/** The raw Ed25519 public key (b64url) of a 32-byte seed. */
function publicFromSeed(seed: Buffer) {
  // PKCS#8 wrapping of an Ed25519 seed: the fixed 16-byte prefix + the seed.
  const der = Buffer.concat([Buffer.from('302e020100300506032b657004220420', 'hex'), seed])
  const pub = createPublicKey(createPrivateKey({ key: der, format: 'der', type: 'pkcs8' }))
  return (pub.export({ format: 'jwk' }) as { x: string }).x
}

/** `Codync-Sig: v=1,kid,ts,nonce,sig` over method, authority, path and the body's hash. */
function signatureHeader(id: Identity, method: string, url: URL, body: Buffer) {
  const ts = Date.now()
  const nonce = b64url(randomBytes(16))
  const input = ['codync-sig-v1', method.toUpperCase(), url.host.toLowerCase(), `${url.pathname || '/'}${url.search}`, String(ts), nonce, b64url(createHash('sha256').update(body).digest())].join('\n')
  const sig = sign(null, Buffer.from(input), id.key)
  return `v=1,kid=${id.publicKey},ts=${ts},nonce=${nonce},sig=${b64url(sig)}`
}

export interface CloudFailure {
  status: number
  code: string
  message: string | null
}

export function registerCloud(account: AccountService) {
  ipcMain.handle('cloud:request', async (_e, method: string, path: string, body: unknown, signed: boolean) => {
    const base = account.state.cloudURL
    const user = account.state.user
    if (!base || !user) return { ok: false, error: { status: 0, code: 'signedOut', message: 'Sign in again to reach your account.' } satisfies CloudFailure }
    const url = new URL(path.replace(/^\//, ''), `${base}/`)
    const payload = body === undefined ? Buffer.alloc(0) : Buffer.from(JSON.stringify(body))
    const headers: Record<string, string> = {}
    try {
      headers.Authorization = `Bearer ${await account.sessionToken()}`
    } catch (error) {
      return { ok: false, error: { status: 401, code: 'signedOut', message: error instanceof Error ? error.message : 'Sign in again.' } }
    }
    if (body !== undefined) headers['Content-Type'] = 'application/json'
    if (signed) headers['Codync-Sig'] = signatureHeader(await identity(user.userId), method, url, payload)
    try {
      const res = await fetch(url, { method, headers, body: body === undefined ? undefined : payload, signal: AbortSignal.timeout(20_000) })
      const text = await res.text()
      const json = text ? (JSON.parse(text) as Record<string, unknown>) : {}
      if (!res.ok) {
        const e = (json.error ?? {}) as { code?: string; message?: string }
        return { ok: false, error: { status: res.status, code: e.code ?? `http${res.status}`, message: e.message ?? null } }
      }
      return { ok: true, value: json }
    } catch {
      return { ok: false, error: { status: 0, code: 'unreachable', message: "Can't reach the Codync cloud. Check your connection." } }
    }
  })
}
