import assert from 'node:assert/strict'
import { test } from 'node:test'
import { computerChoice, firstAmong } from './computer-choice.ts'

const a = { computer: { id: 'a' }, hostName: 'A' }
const b = { computer: { id: 'b' }, hostName: 'B' }
const c = { computer: { id: 'c' }, hostName: 'C' }

test('one computer has no menu; two list both, the shown one picked', () => {
  assert.equal(computerChoice([a], 'a', () => {}), null)
  const choice = computerChoice([a, b], 'b', () => {})
  assert.deepEqual(choice?.options, [{ id: 'a', label: 'A' }, { id: 'b', label: 'B' }])
  assert.equal(choice?.selection, 'b')
})

test('the computer asked for shows while it is offered, else the next preferred, else any', () => {
  assert.equal(firstAmong([a, b], [b, a]), b)
  assert.equal(firstAmong([a], [b, null, a]), a)
  assert.equal(firstAmong([a, b], [c, undefined]), a)
  assert.equal(firstAmong([], [a]), null)
})
