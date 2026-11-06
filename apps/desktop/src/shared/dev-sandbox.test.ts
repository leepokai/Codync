import assert from 'node:assert/strict'
import { test } from 'node:test'
import { devClerkSecretKey, sandboxLabel, sandboxName, sandboxPorts } from './dev-sandbox.ts'

test('sandbox ports skip the ones other sandboxes hold and never reach Codync or Codync Dev', () => {
  const ports = sandboxPorts([19300, 19302])
  assert.deepEqual(ports.slice(0, 2), [19301, 19303])
  assert.equal(ports.length, 98)
  assert.ok(ports.every((port) => port >= 19300 && port <= 19399))
})

test('a sandbox is named after its worktree folder', () => {
  assert.equal(sandboxName('feat details/panel'), 'feat-details-panel')
  assert.equal(sandboxLabel('Codync Dev', '/Users/me/.codync-dev-feat-x'), 'Codync Dev (feat-x)')
  assert.equal(sandboxLabel('Codync Dev', 'C:\\Users\\me\\box\\'), 'Codync Dev (box)')
})

test('a sandbox only ever takes a Clerk development key, from the environment first', () => {
  assert.equal(devClerkSecretKey(undefined, undefined), null)
  assert.equal(devClerkSecretKey(undefined, 'OTHER=1\nCODYNC_CLERK_SECRET_KEY="sk_test_abc"\n'), 'sk_test_abc')
  assert.equal(devClerkSecretKey(' sk_test_env ', 'CODYNC_CLERK_SECRET_KEY=sk_test_file'), 'sk_test_env')
  assert.equal(devClerkSecretKey(undefined, 'CLERK_SECRET_KEY=sk_live_other'), null)
  assert.throws(() => devClerkSecretKey('sk_live_x', undefined), /sk_test_/)
  assert.throws(() => devClerkSecretKey(undefined, 'CODYNC_CLERK_SECRET_KEY=sk_live_x'), /sk_test_/)
})
