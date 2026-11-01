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

const keyPath = (userId: string) => join(app.getPath('userData'), `device-key-${createHash('sha256').update(userId).digest('hex').slice(0, 16)}.bin`)

async function identity(userId: string): Promise<Identity> {
  const cached = identities.get(userId)
  if (cached) return cached
  const path = keyPath(userId)
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

type CloudResult = { ok: true; value: Record<string, unknown> } | { ok: false; error: CloudFailure }

/** One `/v1` call with `token` (a session JWT), signed with `signer`'s device key when given. */
async function cloudCall(base: string, token: string, method: string, path: string, body: unknown, signer: string | null, timeoutMs = 20_000): Promise<CloudResult> {
  const url = new URL(path.replace(/^\//, ''), `${base}/`)
  const payload = body === undefined ? Buffer.alloc(0) : Buffer.from(JSON.stringify(body))
  const headers: Record<string, string> = { Authorization: `Bearer ${token}` }
  if (body !== undefined) headers['Content-Type'] = 'application/json'
  if (signer) headers['Codync-Sig'] = signatureHeader(await identity(signer), method, url, payload)
  try {
    const res = await fetch(url, { method, headers, body: body === undefined ? undefined : payload, signal: AbortSignal.timeout(timeoutMs) })
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
}

/**
 * Signing out: the account drops this install's device, with its grants and requests (iOS and Android
 * `revokeThisDevice`). `token` was taken before Clerk signed out: it stays valid until it expires. Best effort, a short wait.
 */
export async function revokeThisDevice(base: string, token: string, userId: string) {
  if (!identities.has(userId) && !(await fs.stat(keyPath(userId)).catch(() => null))) return
  const publicKey = (await identity(userId)).publicKey
  const list = await cloudCall(base, token, 'GET', 'v1/devices', undefined, null, 5_000)
  const devices = list.ok ? ((list.value.devices ?? []) as { deviceId: string; deviceKey: string }[]) : []
  const device = devices.find((d) => d.deviceKey === publicKey)
  if (device) await cloudCall(base, token, 'DELETE', `v1/devices/${device.deviceId}`, undefined, null, 5_000)
}

/** Leaving an account here (signing out, deleting it): its device key goes, since the cloud never takes a revoked key back. */
export async function dropDeviceKey(userId: string) {
  identities.delete(userId)
  await fs.rm(keyPath(userId), { force: true }).catch(() => {})
}

export function registerCloud(account: AccountService) {
  ipcMain.handle('cloud:request', async (_e, method: string, path: string, body: unknown, signed: boolean): Promise<CloudResult> => {
    const base = account.state.cloudURL
    const user = account.state.user
    if (!base || !user) return { ok: false, error: { status: 0, code: 'signedOut', message: 'Sign in again to reach your account.' } }
    let token: string
    try {
      token = await account.sessionToken()
    } catch (error) {
      return { ok: false, error: { status: 401, code: 'signedOut', message: error instanceof Error ? error.message : 'Sign in again.' } }
    }
    return cloudCall(base, token, method, path, body, signed ? user.userId : null)
  })
}
