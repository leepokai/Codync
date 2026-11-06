import assert from 'node:assert/strict'
import { test } from 'node:test'
import { encodeTemplate } from './bot-template.ts'

test('a template keeps the setup, not the bot identity, fallbacks or who reaches it', () => {
  const encoded = encodeTemplate({
    id: 'b1', name: 'Reviewer', description: '', avatarColor: 'red', avatarShape: 'blob', backend: 'claude',
    cwd: '/src', permission: 'ask', model: null, pinned: true, fallbacks: [], offeredToNetwork: false, reachableByAssistants: false,
  })
  assert.equal(
    encoded,
    '{\n  "avatarColor" : "red",\n  "avatarShape" : "blob",\n  "backend" : "claude",\n  "cwd" : "/src",\n  "description" : "",\n  "name" : "Reviewer",\n  "permission" : "ask"\n}',
  )
})
