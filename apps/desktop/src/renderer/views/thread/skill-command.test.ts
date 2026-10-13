import assert from 'node:assert/strict'
import { test } from 'node:test'
import { matchingSkills, skillQuery } from './skill-command.ts'

test('a leading slash opens the skill menu until the name ends', () => {
  assert.equal(skillQuery('/'), '')
  assert.equal(skillQuery('/pd'), 'pd')
  assert.equal(skillQuery('/pdf merge'), null)
  assert.equal(skillQuery('hi /pdf'), null)
})

test('only the bot\'s own skills match, by id or name', () => {
  const installed = [{ id: 'pdf', name: 'PDF' }, { id: 'code-review', name: 'Code Review' }, { id: 'xlsx', name: 'xlsx' }]
  assert.deepEqual(matchingSkills(installed, ['pdf', 'code-review'], '').map((s) => s.id), ['pdf', 'code-review'])
  assert.deepEqual(matchingSkills(installed, ['pdf', 'code-review'], 'review').map((s) => s.id), ['code-review'])
  assert.deepEqual(matchingSkills(installed, ['pdf'], 'xl'), [])
})
