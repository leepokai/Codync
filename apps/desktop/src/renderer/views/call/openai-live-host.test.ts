import assert from 'node:assert/strict'
import { test } from 'node:test'
import { requireLiveHost } from './openai-live-host.ts'

test('saved Live choices require support from the current host, not the previous host', async () => {
  let status: { supportsLive?: boolean } = { supportsLive: true }
  const methods: string[] = []
  const client = { call: async <T>(method: string): Promise<T> => { methods.push(method); return status as T } }
  await requireLiveHost(client)
  status = {}
  await assert.rejects(requireLiveHost(client), /Update Codync on the computer/)
  status = { supportsLive: false }
  await assert.rejects(requireLiveHost(client), /Update Codync on the computer/)
  assert.deepEqual(methods, ['voiceStatus', 'voiceStatus', 'voiceStatus'])
})
