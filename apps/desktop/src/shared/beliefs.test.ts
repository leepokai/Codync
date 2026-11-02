import assert from 'node:assert/strict'
import { test } from 'node:test'
import { bodyPreview, byAttention, confidenceLabel, confirmedText, filterBeliefs, historyRows, kindLabel, type Belief } from './beliefs.ts'

const belief = (over: Partial<Belief> = {}): Belief => ({
  file: 'home.md', shared: false, description: 'Where the user lives', kind: 'user', confidence: 'medium', confirmed: null,
  updatedAt: 0, tokens: 10, versions: [], ...over,
})

test('beliefs are filtered by kind and by confidence, and null means all', () => {
  const all = [belief({ file: 'a.md' }), belief({ file: 'b.md', kind: 'feedback', confidence: 'low' }), belief({ file: 'c.md', kind: 'project', confidence: 'high' })]
  assert.equal(filterBeliefs(all, { kind: null, confidence: null }).length, 3)
  assert.deepEqual(filterBeliefs(all, { kind: 'feedback', confidence: null }).map((b) => b.file), ['b.md'])
  assert.deepEqual(filterBeliefs(all, { kind: null, confidence: 'high' }).map((b) => b.file), ['c.md'])
  assert.equal(filterBeliefs(all, { kind: 'user', confidence: 'low' }).length, 0)
})

test('the unsure and the long unconfirmed come first', () => {
  const sorted = byAttention([
    belief({ file: 'sure.md', confidence: 'high', confirmed: '2026-10-01' }),
    belief({ file: 'old.md', confirmed: '2026-01-01' }),
    belief({ file: 'new.md', confirmed: '2026-10-01' }),
    belief({ file: 'doubt.md', confidence: 'low', confirmed: '2026-10-30' }),
  ])
  assert.deepEqual(sorted.map((b) => b.file), ['doubt.md', 'old.md', 'new.md', 'sure.md'])
})

test('labels and the confirmation line', () => {
  assert.equal(kindLabel('feedback'), 'How to work')
  assert.equal(confidenceLabel('low'), 'Unsure')
  assert.equal(confirmedText(null), 'Never confirmed')
  assert.equal(confirmedText('2026-10-30'), 'Confirmed 2026-10-30')
})

test('history: the newest cannot be restored, nor can the forgetting itself', () => {
  const rows = historyRows(belief({ versions: [
    { id: 3, content: null, cause: 'delete', at: 30 },
    { id: 2, content: 'x', cause: 'edit', at: 20 },
    { id: 1, content: 'y', cause: 'dream', at: 10 },
  ] }))
  assert.deepEqual(rows.map((r) => [r.id, r.label, r.restorable]), [[3, 'Forgotten', false], [2, 'Changed', true], [1, 'Tidied while dreaming', true]])
})

test('the preview drops the header and cuts long text', () => {
  assert.equal(bodyPreview('---\ndescription: x\ntype: user\n---\nLives in Lisbon.\n'), 'Lives in Lisbon.')
  assert.equal(bodyPreview('word '.repeat(100), 20).length, 20)
})
