import assert from 'node:assert/strict'
import { test } from 'node:test'
import { newestLayer } from './escape-last.ts'

/** A layer whose element reports inert (closest matches) or not. */
const layer = (inert: boolean) => ({
  ref: { current: { closest: () => (inert ? ({} as Element) : null) } as unknown as HTMLElement },
})

test('Escape belongs to the newest layer, not the one under it', () => {
  const details = layer(false)
  const find = layer(false)
  assert.equal(newestLayer([details, find]), find)
  assert.equal(newestLayer([find, details]), details)
})

test('a layer under a modal is inert and leaves the key to the older one', () => {
  const details = layer(false)
  const find = layer(true)
  assert.equal(newestLayer([details, find]), details)
})

test('a layer with no element on the page takes no key', () => {
  assert.equal(newestLayer([{ ref: { current: null } }]), undefined)
  assert.equal(newestLayer([]), undefined)
})
