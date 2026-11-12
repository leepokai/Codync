import assert from 'node:assert/strict'
import { test } from 'node:test'
import { findSpans, joinRuns, matchRanges } from './text-find.ts'

const matchStarts = (text: string, query: string) => matchRanges(text, query).map((m) => m.start)

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

test('a character that lowercases to more than one does not disable the search', () => {
  assert.deepEqual(matchStarts('İstanbul and week', 'week'), [13])
  assert.deepEqual(findSpans([run('İstanbul and week')], 'week'), [{ from: { run: 0, offset: 13 }, to: { run: 0, offset: 17 } }])
  // A match on part of the character covers the whole of it.
  assert.deepEqual(findSpans([run('xİy')], 'i'), [{ from: { run: 0, offset: 1 }, to: { run: 0, offset: 2 } }])
})

test('Greek matches whichever way its sigma is written', () => {
  assert.deepEqual(matchStarts('ΛΟΓΟΣ', 'ΟΣ'), [3])
  assert.deepEqual(matchStarts('ΛΟΓΟΣ', 'ος'), [3])
  assert.deepEqual(matchStarts('λόγος', 'ΟΣ'), [3])
  assert.deepEqual(matchStarts('λογος', 'ΟΣ'), [3])
})

test('nothing to find, nothing found', () => {
  assert.deepEqual(findSpans([run('hello')], 'zzz'), [])
  assert.deepEqual(findSpans([], 'a'), [])
})
