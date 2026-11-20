import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SpeechSegments } from './speech-segments.ts'

/** Results as [text, ends, ms since the one before]; a pause is a second or more. */
const run = (results: [string, boolean, number?][]) => {
  const segments = new SpeechSegments()
  let at = 0
  return results.map(([text, ends, gap = 100]) => segments.heard({ type: 'partial', text, ends }, (at += gap))).at(-1)
}

test('revised words replace the last ones; a transcript that starts over after a pause goes after them', () => {
  assert.equal(run([['I fix', false], ['I fixed the', false], ['I fixed the parser', false]]), 'I fixed the parser')
  assert.equal(run([['I fixed the parser', false], ['I', false, 3000], ['I ran the tests', false]]), 'I fixed the parser I ran the tests')
  // An empty result keeps the words before it.
  assert.equal(run([['I fixed the parser', false], ['', false]]), 'I fixed the parser')
})

test('a revision that shrinks the words is still a revision', () => {
  assert.equal(run([['Three hundred', false], ['300', false], ['300 dollars', false]]), '300 dollars')
  assert.equal(run([['Hi', false], ['I fixed', false]]), 'I fixed')
  // After a pause, a revision keeping the start is the same segment.
  assert.equal(run([['I want to', false], ['I want two tickets', false, 1500]]), 'I want two tickets')
})

test('a short segment before a pause stays', () => {
  assert.equal(run([['Yes', false], ['Run', false, 3000], ['Run the tests', false]]), 'Yes Run the tests')
})

test('after a closed segment, new words are a new segment and the same words again are not', () => {
  assert.equal(run([['OK', true], ['now', false]]), 'OK now')
  assert.equal(run([['in the parser', true], ['in the parser', true]]), 'in the parser')
  assert.equal(run([['in the', true], ['in the parser', false]]), 'in the parser')
  // Only all of the closed words going on are the same segment: a phrase sharing its start is the next one.
  assert.equal(run([['run the tests', true], ['run the build', false]]), 'run the tests run the build')
  // The result that closes a segment, after the pause, is its last revision.
  assert.equal(run([['Three hundred', false], ['300', true, 1500], ['now', false]]), '300 now')
})

test('a final result closes the segment like the end marker', () => {
  const segments = new SpeechSegments()
  segments.heard({ type: 'final', text: 'OK' }, 0)
  assert.equal(segments.heard({ type: 'partial', text: 'now' }, 100), 'OK now')
})
