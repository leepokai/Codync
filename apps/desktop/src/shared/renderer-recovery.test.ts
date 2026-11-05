import assert from 'node:assert/strict'
import { test } from 'node:test'
import { afterPageGone, pageRecovery } from './renderer-recovery.ts'
import type { WindowRecoveryOutcome } from './analytics.ts'

test('a page ended on purpose stays gone without counting as a failure', () => {
  assert.equal(afterPageGone('clean-exit', undefined, 0), null)
  assert.equal(afterPageGone('killed', undefined, 0), null)
})

test('a crashed page reloads twice in a row, then stays down', () => {
  for (const reason of ['crashed', 'oom', 'abnormal-exit', 'memory-eviction']) {
    const first = afterPageGone(reason, undefined, 1000)
    assert.deepEqual(first, { at: 1000, count: 1 })
    const second = afterPageGone(reason, { at: 1000, count: 1 }, 2000)
    assert.deepEqual(second, { at: 2000, count: 2 })
    assert.equal(afterPageGone(reason, { at: 2000, count: 2 }, 3000), 'stay_down')
  }
})

test('a page that stayed up a minute starts the count again', () => {
  assert.equal(afterPageGone('crashed', { at: 0, count: 2 }, 59_999), 'stay_down')
  assert.deepEqual(afterPageGone('crashed', { at: 0, count: 2 }, 60_000), { at: 60_000, count: 1 })
})

/** A recovery with a fake page: what it reported, how often it reloaded, and who waits for the page to load. */
function recovering() {
  const reports: WindowRecoveryOutcome[] = []
  const page = {
    reloads: 0,
    waiting: [] as (() => void)[],
    reload() { this.reloads++ },
    once(_event: 'did-finish-load', listener: () => void) { this.waiting.push(listener) },
  }
  return { reports, page, gone: pageRecovery((outcome) => reports.push(outcome)) }
}

test('a recovery reports once, when its page has loaded, however often the reload crashed', () => {
  const { reports, page, gone } = recovering()
  gone(page, 'crashed')
  gone(page, 'crashed')
  assert.equal(page.reloads, 2)
  assert.deepEqual(reports, [])
  assert.equal(page.waiting.length, 1)
  page.waiting[0]!()
  assert.deepEqual(reports, ['reloaded'])
})

test('a page that keeps crashing stays down and says so', () => {
  const { reports, page, gone } = recovering()
  for (let crash = 0; crash < 3; crash++) gone(page, 'crashed')
  assert.equal(page.reloads, 2)
  assert.deepEqual(reports, ['gave_up'])
})

test('a page ended on purpose is neither reloaded nor reported', () => {
  const { reports, page, gone } = recovering()
  gone(page, 'clean-exit')
  assert.equal(page.reloads, 0)
  assert.deepEqual(reports, [])
})
