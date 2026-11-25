import assert from 'node:assert/strict'
import { test } from 'node:test'
import { matchItems, type PaletteItem } from './palette-items.ts'

const item = (key: string, ...words: string[]): PaletteItem => ({ key, title: words[0] ?? '', leading: null, words, pick: () => {} })

test('an empty query finds everything; a query matches any word, whatever its case', () => {
  const items = [item('a', 'Deploy site', 'Mac mini'), item('b', 'Nightly report', 'Studio')]
  assert.deepEqual(matchItems(items, '  '), items)
  assert.deepEqual(matchItems(items, 'DEPLOY').map((i) => i.key), ['a'])
  assert.deepEqual(matchItems(items, ' studio ').map((i) => i.key), ['b'])
  assert.deepEqual(matchItems(items, 'nothing'), [])
})
