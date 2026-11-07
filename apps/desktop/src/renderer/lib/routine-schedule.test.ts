import assert from 'node:assert/strict'
import { test } from 'node:test'
import { clock, editorKind, toggleDay, usesTime } from './routine-schedule.ts'

test('kinds the editor cannot edit keep their original triggers', () => {
  assert.equal(editorKind('once'), 'keep')
  assert.equal(editorKind('event'), 'event')
  assert.equal(editorKind('webhook'), 'webhook')
  assert.equal(editorKind('interval'), 'interval')
  assert.equal(editorKind('cron'), 'cron')
})

test('toggling a weekday keeps the chosen days sorted', () => {
  assert.deepEqual(toggleDay([1, 5], 3), [1, 3, 5])
  assert.deepEqual(toggleDay([1, 5], 1), [5])
})

test('schedule times wrap into valid hours and minutes', () => {
  assert.equal(clock(9, 5), '09:05')
  assert.equal(clock(-1, -1), '23:59')
  assert.equal(usesTime('weekly'), true)
  assert.equal(usesTime('hourly'), false)
})
