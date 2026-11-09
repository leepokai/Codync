import assert from 'node:assert/strict'
import { test } from 'node:test'
import { hasNoReading, pastMiddle, servicesIn } from './usage-order.ts'

const service = (id: string, shown = true) => ({ id, name: id, shown })

test('a move on its way holds the order and agents it does not name follow', () => {
  const usage = { services: ['claude', 'codex', 'cursor'].map((id) => service(id)) }
  const ids = (order: string[] | null) => servicesIn(usage, order).map((s) => s.id)
  assert.deepEqual(ids(null), ['claude', 'codex', 'cursor'])
  assert.deepEqual(ids(['cursor', 'gone', 'claude']), ['cursor', 'claude', 'codex'])
  assert.deepEqual(servicesIn({}, ['claude']), [])
})

test('only an agent that is on can lack a reading', () => {
  const usage = { providers: [{ id: 'claude', name: 'Claude', windows: [], source: 'claude', updatedAt: 1 }] }
  assert.equal(hasNoReading(usage, service('claude')), false)
  assert.equal(hasNoReading(usage, service('cursor')), true)
  assert.equal(hasNoReading(usage, service('cursor', false)), false)
})

test('a dragged row takes a place only past its middle', () => {
  const box = { top: 100, height: 40 }
  assert.equal(pastMiddle(true, 119, box), false)
  assert.equal(pastMiddle(true, 120, box), true)
  assert.equal(pastMiddle(false, 121, box), false)
  assert.equal(pastMiddle(false, 120, box), true)
})
