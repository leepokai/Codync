import assert from 'node:assert/strict'
import { test } from 'node:test'
import { closeTab, openTab } from './tabs.ts'

test('a workflow opens as one tab, at the end', () => {
  assert.deepEqual(openTab(['a', 'b'], 'c'), ['a', 'b', 'c'])
  assert.deepEqual(openTab(['a', 'b'], 'a'), ['a', 'b'])
})

test('closing a tab shows the one after it, or the one before at the end', () => {
  assert.deepEqual(closeTab(['a', 'b', 'c'], 'b'), { tabs: ['a', 'c'], next: 'c' })
  assert.deepEqual(closeTab(['a', 'b', 'c'], 'c'), { tabs: ['a', 'b'], next: 'b' })
  assert.deepEqual(closeTab(['a'], 'a'), { tabs: [], next: null })
})
