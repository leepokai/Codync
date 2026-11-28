import assert from 'node:assert/strict'
import { test } from 'node:test'
import { LEFTOVER } from './model-manager-leftovers.ts'

// The four names `model-manager.spec.ts` creates, with the timestamp it appends. If this filter stops
// matching them, `model-manager-cleanup.spec.ts` finds nothing, deletes nothing, and its poll still
// passes on an empty list: the cleanup would be a no-op nobody notices.
const CREATED = ['Bot manager e2e A 1733097600000', 'Bot manager e2e B 1733097600000', 'Provider custom 1733097600000', 'Provider agent 1733097600000']

test('the leftover filter matches each name model-manager.spec.ts creates', () => {
  for (const name of CREATED) assert.ok(LEFTOVER.test(name), `${name} must be a leftover`)
})

test('the leftover filter leaves other bots alone', () => {
  const kept = ['Bot manager e2e C 1733097600000', 'Provider other 1733097600000', 'Bot manager e2e A', 'my bot', 'Provider custom 12a']
  for (const name of kept) assert.ok(!LEFTOVER.test(name), `${name} must not be swept`)
})
