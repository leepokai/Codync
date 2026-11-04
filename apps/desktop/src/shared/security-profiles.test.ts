import assert from 'node:assert/strict'
import { test } from 'node:test'
import { previewLines, profileRules, type SecurityProfile } from './security-profiles.ts'

const readOnly: SecurityProfile = {
  id: 'readOnly',
  title: 'Read-only',
  summary: 'Reads and searches on its own.',
  permission: 'ask',
  budget: 300000,
  rules: [
    { kind: 'read', decision: 'allow' },
    { kind: 'search', decision: 'allow' },
    { kind: 'edit', decision: 'ask' },
    { kind: 'execute', decision: 'deny' },
  ],
}

test('a profile writes one catch-all rule per tool kind, in the host\'s order', () => {
  assert.deepEqual(profileRules(readOnly).map((r) => `${r.kind}:${r.decision}:${r.pattern}`), [
    'read:allow:*',
    'search:allow:*',
    'edit:ask:*',
    'execute:deny:*',
  ])
})

test('the preview says what is allowed, asked and refused, and the limit', () => {
  assert.deepEqual(previewLines(readOnly), [
    'Allows: read, search',
    'Asks before: edit',
    'Refuses: execute',
    'Everything else: asks you',
    'Daily limit: 300,000 tokens',
    'Commands no bot may run stay refused under every profile.',
  ])
})

test('full access has no rules and no limit, and still names the forbidden commands', () => {
  const full: SecurityProfile = { ...readOnly, id: 'full', permission: 'auto', budget: null, rules: [] }
  assert.deepEqual(previewLines(full), [
    'Everything else: approved automatically',
    'Commands no bot may run stay refused under every profile.',
  ])
})
