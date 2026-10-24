import assert from 'node:assert/strict'
import { test } from 'node:test'
import { sandboxLabel, sandboxName, sandboxPorts } from './dev-sandbox.ts'

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
