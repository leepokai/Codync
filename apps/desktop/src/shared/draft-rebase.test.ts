import assert from 'node:assert/strict'
import { test } from 'node:test'
import { rebase } from './draft-rebase.ts'
import type { BotDraft } from './models.ts'

test('a settings form takes changes made elsewhere and keeps its own edits', () => {
  const base: BotDraft = { id: 'b', name: 'Lead', description: '', avatarColor: 'red', avatarShape: 'blob', backend: 'claude', cwd: '', permission: 'ask', model: 'sonnet', notify: null }
  const draft = { ...base, notify: false }
  const fresh = { ...base, model: 'opus', name: 'Boss', manageTeam: true }
  assert.deepEqual(rebase(draft, base, fresh), { ...fresh, notify: false })
  // A field edited here stays, even when the host changed it too.
  assert.equal(rebase({ ...draft, model: 'haiku' }, base, fresh).model, 'haiku')
  assert.equal(rebase(draft, base, base), draft, 'nothing moved')
})
