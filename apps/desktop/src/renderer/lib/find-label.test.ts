import assert from 'node:assert/strict'
import { test } from 'node:test'
import { findLabel } from './find-label.ts'

test('the count reads active over total once something matched', () => {
  assert.equal(findLabel('hi', { active: 2, matches: 7 }, 'No results'), '2/7')
})

test('no match says so, and nothing searched says nothing', () => {
  assert.equal(findLabel('zzz', { active: 0, matches: 0 }, 'No results'), 'No results')
  assert.equal(findLabel('', { active: 0, matches: 0 }, 'No results'), '')
  assert.equal(findLabel('  ', { active: 1, matches: 1 }, 'No results'), '')
})
