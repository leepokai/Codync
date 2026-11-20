import assert from 'node:assert/strict'
import { test } from 'node:test'
import { KOFI_ASK_DAYS, countDay, dueKofiAsk, kofiAskAtOpening } from './kofi-ask.ts'

test('a day counts once, however many prompts it has', () => {
  const first = countDay({ count: 0, last: '' }, '2026-11-17')
  assert.deepEqual(first, { count: 1, last: '2026-11-17' })
  assert.equal(countDay(first, '2026-11-17'), first)
  assert.deepEqual(countDay(first, '2026-11-20'), { count: 2, last: '2026-11-20' })
})

test('the invitation comes at 3, 30 and 90 active days, then never again', () => {
  assert.equal(dueKofiAsk({ asked: 0, day: 0 }, 2), null)
  assert.equal(dueKofiAsk({ asked: 0, day: 0 }, 3), 1)
  assert.equal(dueKofiAsk({ asked: 1, day: 3 }, 29), null)
  assert.equal(dueKofiAsk({ asked: 1, day: 3 }, 30), 2)
  assert.equal(dueKofiAsk({ asked: 2, day: 30 }, 89), null)
  assert.equal(dueKofiAsk({ asked: 2, day: 30 }, 90), 3)
  assert.equal(dueKofiAsk({ asked: 3, day: 90 }, 1000), null)
})

test('a late invitation never brings the next one sooner, and opening Ko-fi ends them', () => {
  assert.equal(dueKofiAsk({ asked: 0, day: 0 }, 40), 1)
  assert.equal(dueKofiAsk({ asked: 1, day: 40 }, 41), null, 'not at the next opening')
  assert.equal(dueKofiAsk({ asked: 1, day: 40 }, 67), 2)
  assert.equal(dueKofiAsk({ asked: KOFI_ASK_DAYS.length, day: 3 }, 90), null)
})

test("an opening with What's new up or a star ask due leaves it for a later one", () => {
  const due = { asked: 0, day: 0 }
  assert.equal(kofiAskAtOpening(due, 3, { held: false, starDue: false }), 1)
  assert.equal(kofiAskAtOpening(due, 3, { held: true, starDue: false }), null)
  assert.equal(kofiAskAtOpening(due, 3, { held: false, starDue: true }), null)
})
