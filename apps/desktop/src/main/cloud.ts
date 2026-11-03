import { createHash, generateKeyPairSync, randomBytes, sign } from 'node:crypto'
import { promises as fs } from 'node:fs'
import { join } from 'node:path'
import { app, ipcMain, safeStorage } from 'electron'
import type { AccessTicket, AccountComputer, CallError } from '../shared/ipc'
import type { AccountService } from './account'
import { closeAccountChannels, registerAccountChannels } from './account-channel'
import { computerId, sasCode, sasCommit, signingKey, type DeviceKey } from './channel-crypto'

/**
 * The Codync cloud's user-facing `/v1` API (§8.4) with this install's device key (§3.2): an
 * Ed25519 key per account, kept encrypted with the OS keychain, that signs `Codync-Sig` (§5).
 */

const b64url = (b: Buffer) => b.toString('base64url')

const identities = new Map<string, DeviceKey>()

const keyPath = (userId: string) => join(app.getPath('userData'), `device-key-${createHash('sha256').update(userId).digest('hex').slice(0, 16)}.bin`)

async function identity(userId: string): Promise<DeviceKey> {
  const cached = identities.get(userId)
  if (cached) return cached
  const path = keyPath(userId)
  let seed: Buffer | null = null
  try {
    const sealed = await fs.readFile(path)
    seed = Buffer.from(safeStorage.isEncryptionAvailable() ? safeStorage.decryptString(sealed) : sealed.toString('utf8'), 'base64url')
  } catch {}
  if (seed?.length !== 32) {
    const jwk = generateKeyPairSync('ed25519').privateKey.export({ format: 'jwk' })
    seed = Buffer.from(jwk.d ?? '', 'base64url')
    const raw = safeStorage.isEncryptionAvailable() ? safeStorage.encryptString(b64url(seed)) : Buffer.from(b64url(seed))
    await fs.mkdir(app.getPath('userData'), { recursive: true })
    await fs.writeFile(path, raw, { mode: 0o600 })
  }
  const made = signingKey(seed)
  identities.set(userId, made)
  return made
}

/** `Codync-Sig: v=1,kid,ts,nonce,sig` over method, authority, path and the body's hash. */
function signatureHeader(id: DeviceKey, method: string, url: URL, body: Buffer) {
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

/**
 * Asks `computer` to let this install in through the account (iOS and Android `requestAccess`): commits to
 * a nonce, waits for the computer's, reveals ours, and returns the 6-digit code both sides show.
 */
async function requestAccess(call: (method: string, path: string, body?: unknown) => Promise<CloudResult>, device: DeviceKey, computer: AccountComputer): Promise<AccessTicket> {
  if (computerId(computer.signKey) !== computer.id) throw new Error("This computer's key doesn't match its ID.")
  const nonce = randomBytes(32)
  const created = await call('POST', `v1/computers/${computer.id}/access-requests`, { commit: sasCommit(device.publicKey, nonce) })
  if (!created.ok) throw new Error(created.error.message ?? "The computer didn't take the request.")
  const requestId = String(created.value.requestId)
  const expiresAt = Number(created.value.expiresAt)
  try {
    for (;;) {
      if (Date.now() >= expiresAt) throw new Error('The request expired. Ask again.')
      const status = await call('GET', `v1/access-requests/${requestId}`)
      if (status.ok && status.value.status !== 'pending') throw new Error("The computer didn't take the request.")
      if (status.ok && typeof status.value.hostNonce === 'string') {
        const revealed = await call('POST', `v1/access-requests/${requestId}/reveal`, { nonce: b64url(nonce) })
        if (!revealed.ok) throw new Error(revealed.error.message ?? "The computer didn't take the request.")
        return { requestId, expiresAt, code: sasCode(computer.signKey, device.publicKey, nonce, status.value.hostNonce) }
      }
      await new Promise((resolve) => setTimeout(resolve, 2_000))
    }
  } catch (error) {
    // A failed nonce exchange leaves no invisible request waiting at the computer.
    await call('DELETE', `v1/access-requests/${requestId}`)
    throw error
  }
}

export function registerCloud(account: AccountService) {
  const session = async () => {
    const base = account.state.cloudURL
    const user = account.state.user
    if (!base || !user) throw { status: 0, code: 'signedOut', message: 'Sign in again to reach your account.' } satisfies CloudFailure
    try {
      return { base, userId: user.userId, token: await account.sessionToken() }
    } catch (error) {
      throw { status: 401, code: 'signedOut', message: error instanceof Error ? error.message : 'Sign in again.' } satisfies CloudFailure
    }
  }
  ipcMain.handle('cloud:request', async (_e, method: string, path: string, body: unknown, signed: boolean): Promise<CloudResult> => {
    try {
      const { base, userId, token } = await session()
      return await cloudCall(base, token, method, path, body, signed ? userId : null)
    } catch (error) {
      return { ok: false, error: error as CloudFailure }
    }
  })
  ipcMain.handle('cloud:requestAccess', async (_e, computer: AccountComputer) => {
    try {
      const { base, userId, token } = await session()
      const call = (method: string, path: string, body?: unknown) => cloudCall(base, token, method, path, body, userId)
      return { ok: true, value: await requestAccess(call, await identity(userId), computer) }
    } catch (error) {
      return { ok: false, error: { status: 0, message: error instanceof Error ? error.message : (error as CloudFailure).message ?? 'Sign in again.' } }
    }
  })
  let user = account.state.user?.userId
  account.on('change', () => {
    if (account.state.user?.userId === user) return
    user = account.state.user?.userId
    closeAccountChannels()
  })
  const device = () => {
    const userId = account.state.user?.userId
    return userId ? identity(userId) : Promise.reject({ status: 401, message: 'Sign in again to reach your account.' } satisfies CallError)
  }
  registerAccountChannels({
    relay: () => (account.state.user ? account.state.cloudURL : null),
    device,
    sign: async (url) => signatureHeader(await device(), 'GET', url, Buffer.alloc(0)),
  })
}
