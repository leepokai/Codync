import assert from 'node:assert/strict'
import { test } from 'node:test'
import { approvable, inboxTitle, stepsPreview, toolsText } from './skill-proposals.ts'

test('the inbox title counts what waits', () => {
  assert.equal(inboxTitle(0), 'Proposed skills')
  assert.equal(inboxTitle(3), 'Proposed skills (3)')
})

test('tools read as words, a command as its program', () => {
  assert.equal(toolsText(['edit', 'execute:cargo', 'read']), 'edit, cargo, read')
  assert.equal(toolsText(['execute:']), 'commands')
})

test('the card shows the first numbered steps', () => {
  const text = 'Use this when: publishing\n\n1. Test\n2. Tag\n3. Push\n4. Announce\n'
  assert.deepEqual(stepsPreview(text), ['1. Test', '2. Tag', '3. Push'])
  assert.deepEqual(stepsPreview('no steps here'), [])
})

test('a proposal needs all three parts to be approved', () => {
  assert.ok(approvable({ name: 'a', description: 'b', instructions: 'c' }))
  assert.ok(!approvable({ name: ' ', description: 'b', instructions: 'c' }))
  assert.ok(!approvable({ name: 'a', description: '', instructions: 'c' }))
})
