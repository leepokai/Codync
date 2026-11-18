import assert from 'node:assert/strict'
import { test } from 'node:test'
import { appendPage, emptyPage, replacePage, skillCopyLabel } from './market-models.ts'

test('catalog pages keep every fetched row and drop duplicates', () => {
  const first = replacePage([0, 1, 2], (n) => String(n))
  const next = appendPage(first, [2, 3, 3], (n) => String(n))
  assert.deepEqual(next.items, [0, 1, 2, 3])
  assert.deepEqual(replacePage([], (n: number) => String(n)).items, emptyPage().items)
})

test('skills that share a name are told apart by id, the others carry no label', () => {
  const skills = [
    { id: 'humanizer', name: 'Humanizer' },
    { id: 'humanizer-2', name: 'Humanizer' },
    { id: 'pdf', name: 'PDF' },
  ]
  assert.deepEqual(skills.map((s) => skillCopyLabel(skills, s)), ['humanizer', 'humanizer-2', ''])
  assert.equal(skillCopyLabel([], skills[0]), '')
})
