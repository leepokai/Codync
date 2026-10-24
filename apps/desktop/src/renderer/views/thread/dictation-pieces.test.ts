import assert from 'node:assert/strict'
import { test } from 'node:test'
import { inOrder, joinDictated, MIN_SAMPLES, PIECE_SAMPLES, pieces } from './dictation-pieces.ts'

const recording = (samples: number, chunk = 4096) => {
  const chunks: Int16Array[] = []
  for (let at = 0; at < samples; at += chunk) chunks.push(Int16Array.from({ length: Math.min(chunk, samples - at) }, (_, i) => (at + i) % 30000))
  return chunks
}
const lengths = (samples: number) => pieces(recording(samples)).map((p) => p.length)

test('a recording is cut into 20 s pieces that keep every sample in order', () => {
  assert.deepEqual(lengths(PIECE_SAMPLES * 0.5), [PIECE_SAMPLES * 0.5])
  assert.deepEqual(lengths(PIECE_SAMPLES * 2), [PIECE_SAMPLES, PIECE_SAMPLES])
  assert.deepEqual(lengths(PIECE_SAMPLES * 2 + MIN_SAMPLES), [PIECE_SAMPLES, PIECE_SAMPLES, MIN_SAMPLES])
  const joined = pieces(recording(PIECE_SAMPLES * 2 + 123)).flatMap((p) => [...p])
  assert.deepEqual(joined, recording(PIECE_SAMPLES * 2 + 123).flatMap((c) => [...c]))
})

test('a tail under half a second stays with the piece before', () => {
  assert.deepEqual(lengths(PIECE_SAMPLES + MIN_SAMPLES - 1), [PIECE_SAMPLES + MIN_SAMPLES - 1])
  assert.deepEqual(lengths(PIECE_SAMPLES * 2 + 1), [PIECE_SAMPLES, PIECE_SAMPLES + 1])
})

test('transcripts come back in order with a few requests at a time', async () => {
  let running = 0
  let most = 0
  const out = await inOrder([5, 1, 4, 2, 3, 0], async (n) => {
    most = Math.max(most, ++running)
    await new Promise((done) => setTimeout(done, n))
    running--
    return `piece ${n}`
  })
  assert.deepEqual(out, ['piece 5', 'piece 1', 'piece 4', 'piece 2', 'piece 3', 'piece 0'])
  assert.equal(most, 3)
})

test('the first failed piece stops the rest', async () => {
  const started: number[] = []
  await assert.rejects(inOrder([0, 1, 2, 3, 4, 5, 6], async (n) => {
    started.push(n)
    if (n === 0) throw new Error('429')
    await new Promise((done) => setTimeout(done, 5))
  }), /429/)
  assert.deepEqual(started, [0, 1, 2])
})

test('dictated words go after the box with one space', () => {
  assert.equal(joinDictated('', 'hello'), 'hello')
  assert.equal(joinDictated('Fix the bug', 'in the parser'), 'Fix the bug in the parser')
  assert.equal(joinDictated('Fix the bug\n', 'now'), 'Fix the bug\nnow')
  assert.equal(joinDictated('Fix the bug', ''), 'Fix the bug')
})
