import assert from 'node:assert/strict'
import { test } from 'node:test'
import { findSpans, joinRuns, matchStarts } from './text-find.ts'

const run = (text: string, block: unknown = 'p') => ({ text, block })

test('matching ignores case and counts each place once', () => {
  assert.deepEqual(matchStarts('Week week WEEK', 'week'), [0, 5, 10])
  assert.deepEqual(matchStarts('aaaa', 'aa'), [0, 2])
  assert.deepEqual(matchStarts('anything', ''), [])
})

test('a match can cross from one run into the next in the same block', () => {
  const runs = [run('Fix the iso'), run('Week', 'p'), run(' bug')]
  assert.deepEqual(findSpans(runs, 'isoweek'), [{ from: { run: 0, offset: 8 }, to: { run: 1, offset: 4 } }])
})

test('a match stays inside its run and ends where it ends', () => {
  const [span] = findSpans([run('hello world')], 'world')
  assert.deepEqual(span, { from: { run: 0, offset: 6 }, to: { run: 0, offset: 11 } })
})

test('different blocks are kept apart', () => {
  const runs = [run('the end', 'a'), run('start of next', 'b')]
  assert.equal(joinRuns(runs).text, 'the end\nstart of next')
  assert.deepEqual(findSpans(runs, 'endstart'), [])
  assert.equal(findSpans(runs, 'end').length, 1)
})

test('nothing to find, nothing found', () => {
  assert.deepEqual(findSpans([run('hello')], 'zzz'), [])
  assert.deepEqual(findSpans([], 'a'), [])
})
