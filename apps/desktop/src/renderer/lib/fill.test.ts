import assert from 'node:assert/strict'
import { test } from 'node:test'
import { fill } from './fill.ts'

test('what fill puts in stays as written', () => {
  assert.equal(fill('{name} and {other}', { name: 'Build {other} $&', other: 'Reviewer' }), 'Build {other} $& and Reviewer')
  assert.equal(fill('{name} and {missing}', { name: 'Build' }), 'Build and {missing}')
})
