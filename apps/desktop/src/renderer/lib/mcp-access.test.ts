import assert from 'node:assert/strict'
import { test } from 'node:test'
import { activeClients, clientDetail, daysLeft, readMcpAccess } from './mcp-access.ts'

test('reads the apps a computer lets in, leaving out what it cannot read and the revoked ones', () => {
  const now = Date.UTC(2026, 10, 27)
  const access = readMcpAccess({
    clients: [
      { id: 'a', label: 'Laptop agent', access: 'auto', runs: true, createdAt: 1, expiresAt: now + 3 * 86_400_000 },
      { id: 'b', label: 'Old', access: 'full', createdAt: 2, expiresAt: now, revokedAt: 5 },
      { label: 'no id' },
      { id: 'c', label: 'Odd level', access: 'root', createdAt: 3, expiresAt: now - 1 },
    ],
    levels: { max: 'full' },
  })
  assert.deepEqual(activeClients(access).map((c) => c.id), ['c', 'a'])
  assert.equal(access.clients.find((c) => c.id === 'c')?.access, 'readOnly', 'an unknown level reads as the least')
  assert.deepEqual(access.clients.map((c) => c.runs), [true, false, false], 'only the switched-on app runs routines and workflows')
  assert.deepEqual(access.levels, { max: 'full', local: 'ask', cloud: 'readOnly' })
  const tr = (t: string) => t
  assert.equal(daysLeft(access.clients[0]!, now), 3)
  assert.equal(clientDetail(access.clients[0]!, tr, now), 'Expires in 3 days')
  assert.equal(clientDetail(access.clients[2]!, tr, now), 'Expired')
  assert.deepEqual(readMcpAccess(undefined).clients, [], 'an older host has none')
})
