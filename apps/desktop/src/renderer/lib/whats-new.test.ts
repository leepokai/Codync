import assert from 'node:assert/strict'
import { test } from 'node:test'
import { due, type Announcement } from './whats-new.ts'

const a = (version: string): Announcement => ({ version, notes: [`Notes for ${version}`] })
const versions = (list: Announcement[]) => list.map((x) => x.version)
const all = [a('2.46.0'), a('2.44.0'), a('2.40.0')]

test('a new install records its version and shows nothing', () => {
  assert.deepEqual(due(all, null, '2.46.0', false), [])
})

test('a device set up before the feature counts from 0.0.0', () => {
  assert.deepEqual(due(all, null, '2.44.0', true), [a('2.44.0'), a('2.40.0')])
})

test('only versions after seen and up to current are due', () => {
  assert.deepEqual(versions(due(all, '2.43.2', '2.44.0', false)), ['2.44.0'], 'seen older')
  assert.deepEqual(versions(due(all, '2.44.0', '2.44.0', false)), [], 'seen equal')
  assert.deepEqual(versions(due(all, '2.45.0', '2.44.0', false)), [], 'seen newer: a downgrade shows nothing')
  assert.deepEqual(versions(due(all, '2.44.0', '2.45.1', false)), [], 'a later version without an announcement')
})

test('versions compare as numbers, not text', () => {
  assert.deepEqual(versions(due([a('2.10.0')], '2.9.0', '2.10.0', false)), ['2.10.0'])
})
