import { test } from 'node:test'
import assert from 'node:assert/strict'
import { approvals, withApproval } from './approvals.ts'
import { withBackend } from './fallbacks.ts'
import type { BotDraft } from './models.ts'

const codex = {
  modes: [{ id: 'auto', name: 'Auto review', description: 'Only ask for actions detected as potentially unsafe' }, { id: 'read-only', name: 'Read-only' }],
}
const labels = (draft: Pick<BotDraft, 'permission' | 'mode'>, catalog: typeof codex | { modes: [] } | null) =>
  approvals(draft, catalog).choices.map((c) => c.label)

test("approvals list the agent's modes, Auto first, then approve automatically", () => {
  const draft = { permission: 'ask', mode: 'auto' as string | null }
  const { choices, selected } = approvals(draft, codex)
  assert.deepEqual(choices.map((c) => c.label), ['Auto review', 'Read-only', 'Approve automatically'])
  assert.equal(selected.label, 'Auto review')
  assert.equal(selected.note, 'Only ask for actions detected as potentially unsafe')
  assert.deepEqual(withApproval(draft, choices[1]!.id), { permission: 'ask', mode: 'read-only' })
  const approve = withApproval(draft, choices[2]!.id)
  assert.deepEqual(approve, { permission: 'auto', mode: null })
  assert.equal(JSON.stringify(approve), '{"permission":"auto","mode":null}', 'null reaches the host and clears it')
  assert.equal(approvals(approve, codex).selected.label, 'Approve automatically')
})

test('a bot never shows as more careful than it is', () => {
  // Without a mode it works in the agent's default, and asks.
  assert.equal(approvals({ permission: 'ask', mode: null }, codex).selected.label, 'Ask me')
  assert.deepEqual(labels({ permission: 'ask', mode: null }, codex), ['Auto review', 'Read-only', 'Ask me', 'Approve automatically'])
  // A mode the list lacks (or that hasn't loaded) shows as itself.
  assert.deepEqual(labels({ permission: 'ask', mode: 'bypassPermissions' }, null), ['Auto', 'Ask me', 'bypassPermissions', 'Approve automatically'])
  assert.equal(approvals({ permission: 'ask', mode: 'bypassPermissions' }, null).selected.label, 'bypassPermissions')
})

test('an agent without Auto asks, and one still loading offers Auto', () => {
  const plain = { modes: [] as [] }
  assert.deepEqual(labels({ permission: 'ask', mode: 'auto' }, plain), ['Ask me', 'Approve automatically'])
  assert.equal(approvals({ permission: 'ask', mode: 'auto' }, plain).selected.label, 'Ask me')
  assert.deepEqual(labels({ permission: 'ask', mode: 'auto' }, null), ['Auto', 'Ask me', 'Approve automatically'])
})

test("another agent's mode ids mean nothing: a picked mode becomes its Auto mode", () => {
  const draft: BotDraft = { name: 'B', description: '', avatarColor: 'red', avatarShape: 'blob', backend: 'claude', cwd: '', permission: 'ask', mode: 'plan' }
  assert.equal(withBackend(draft, 'codex').mode, 'auto')
  assert.equal(withBackend({ ...draft, mode: null }, 'codex').mode, null)
})
