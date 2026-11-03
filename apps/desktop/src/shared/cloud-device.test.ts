import assert from 'node:assert/strict'
import * as crypto from 'node:crypto'
import path from 'node:path'
import { test } from 'node:test'
import { loadMainModule } from './test-support/main-module.ts'

// The key files, in memory; `safeStorage` is unavailable, so a file holds the seed as base64url text.
const files = new Map<string, Buffer>()
const missing = () => Promise.reject(Object.assign(new Error('ENOENT'), { code: 'ENOENT' }))
const fs = {
  readFile: async (file: string) => files.get(file) ?? missing(),
  writeFile: async (file: string, data: Buffer) => { files.set(file, Buffer.from(data)) },
  mkdir: async () => {},
  stat: async (file: string) => (files.has(file) ? {} : missing()),
  rm: async (file: string) => { files.delete(file) },
}
const calls: string[] = []
const { revokeThisDevice, dropDeviceKey } = loadMainModule<typeof import('../main/cloud.ts')>('cloud', {
  'node:crypto': crypto, 'node:fs': { promises: fs }, 'node:path': path,
  electron: { app: { getPath: () => '/data' }, ipcMain: {}, safeStorage: { isEncryptionAvailable: () => false } },
}, {
  URL, Buffer,
  fetch: async (input: URL, init: RequestInit) => {
    calls.push(`${init.method} ${input.pathname} ${(init.headers as Record<string, string>).Authorization}`)
    return new Response(input.pathname === '/v1/devices'
      ? JSON.stringify({ devices: [{ deviceId: 'other', deviceKey: 'elsewhere' }, { deviceId: 'mine', deviceKey: publicKey }] })
      : '{}')
  },
})
const { d: seed, x: publicKey } = crypto.generateKeyPairSync('ed25519').privateKey.export({ format: 'jwk' }) as { d: string; x: string }

test("signing out revokes only this install's device, then its key goes", async () => {
  await revokeThisDevice('https://cloud.test', 'jwt', 'user')
  assert.deepEqual(calls, [], 'no key here: nothing to revoke')

  const keyFile = `/data/device-key-${crypto.createHash('sha256').update('user').digest('hex').slice(0, 16)}.bin`
  files.set(keyFile, Buffer.from(seed))
  await revokeThisDevice('https://cloud.test', 'jwt', 'user')
  assert.deepEqual(calls, ['GET /v1/devices Bearer jwt', 'DELETE /v1/devices/mine Bearer jwt'])

  await dropDeviceKey('user')
  assert.equal(files.has(keyFile), false)
  calls.length = 0
  await revokeThisDevice('https://cloud.test', 'jwt', 'user')
  assert.deepEqual(calls, [], 'the dropped key is gone from memory too')
})
