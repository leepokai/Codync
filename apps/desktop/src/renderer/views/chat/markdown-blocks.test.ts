import assert from 'node:assert/strict'
import { test } from 'node:test'
import { parseMarkdown } from './markdown-blocks.ts'

test('blocks: headings, nested lists, rules, tables and code', () => {
  const blocks = parseMarkdown('# Title\n- one\n  - two\n1. first\n---\n| A | B |\n|---|---|\n| 1 | 2 |\n```js\nx = 1\n```\n> quoted')
  assert.deepEqual(blocks, [
    { kind: 'heading', text: 'Title', level: 1 },
    { kind: 'bullet', text: 'one', marker: '•', depth: 0 },
    { kind: 'bullet', text: 'two', marker: '◦', depth: 1 },
    { kind: 'bullet', text: 'first', marker: '1.', depth: 0 },
    { kind: 'rule' },
    { kind: 'table', header: ['A', 'B'], rows: [['1', '2']] },
    { kind: 'code', text: 'x = 1', language: 'js' },
    { kind: 'quote', text: 'quoted' },
  ])
})

test('a pipe line without its rule stays text', () => {
  assert.deepEqual(parseMarkdown('| not a table'), [{ kind: 'paragraph', text: '| not a table' }])
})
