import assert from 'node:assert/strict'
import { test } from 'node:test'
import { connectTimedOut, maxRecoveryAttempts, recover, retryDelay, Unreachable, type Failure, type Recovery } from './screen-recovery.ts'

function harness(reconnect: () => Promise<void>, route: () => Promise<void> = async () => {}) {
  const log: string[] = []
  const state = { attempts: 0, failed: null as Failure | null, open: true }
  const r: Recovery = {
    active: () => state.open && state.failed === null,
    route,
    reconnect: async () => { log.push('reconnect'); await reconnect() },
    sleep: async (ms) => { log.push(`sleep ${ms}`) },
    fail: (failure) => { state.failed = failure },
  }
  return { r, state, log }
}

test('after ICE fails the viewer reconnects at most three times, then reports the error', async () => {
  const { r, state, log } = harness(async () => { throw new Error('helper restarting') })
  await recover(r, state, 0, () => 'helper restarting')
  assert.equal(log.filter((l) => l === 'reconnect').length, maxRecoveryAttempts)
  assert.deepEqual(state.failed, { reason: 'host', detail: 'helper restarting' })
  assert.deepEqual(log.filter((l) => l.startsWith('sleep')), ['sleep 0', 'sleep 2000', 'sleep 4000'])
})

test('a reconnect that works ends recovery with attempts left untouched for the session to reset', async () => {
  let calls = 0
  const { r, state } = harness(async () => { if (++calls < 2) throw new Error('busy') })
  await recover(r, state, 0, () => null)
  assert.equal(calls, 2)
  assert.equal(state.failed, null)
  assert.equal(state.attempts, 2)
})

test('attempts already spent count: a connection that never went live does not get a fresh three', async () => {
  const { r, state, log } = harness(async () => { throw new Error('x') })
  state.attempts = 2
  await recover(r, state, 0, () => null)
  assert.equal(log.filter((l) => l === 'reconnect').length, 1)
  assert.ok(state.failed)
  const spent = harness(async () => {})
  spent.state.attempts = 3
  await recover(spent.r, spent.state, 0, () => 'Lost it')
  assert.deepEqual(spent.state.failed, { reason: 'host', detail: 'Lost it' })
  const silent = harness(async () => {})
  silent.state.attempts = 3
  await recover(silent.r, silent.state, 0, () => null)
  assert.deepEqual(silent.state.failed, { reason: 'reconnect', detail: null }, 'no host text: this app\'s own wording')
  assert.deepEqual(spent.log.filter((l) => l === 'reconnect'), [])
})

test('a denial or a required update ends recovery on the spot', async () => {
  for (const status of [403, 426]) {
    const { r, state, log } = harness(async () => { throw Object.assign(new Error('Not allowed'), { status }) })
    await recover(r, state, 0, () => null)
    assert.equal(log.filter((l) => l === 'reconnect').length, 1, String(status))
    assert.deepEqual(state.failed, { reason: 'host', detail: 'Not allowed' })
  }
})

test('an unreachable computer waits for its route without spending an attempt; a lost access fails', async () => {
  let ready = false
  const waited = harness(async () => {}, async () => { if (!ready) { ready = true; await Promise.resolve() } })
  await recover(waited.r, waited.state, 0, () => null)
  assert.equal(waited.state.attempts, 1)
  const denied = harness(async () => {}, async () => { throw Object.assign(new Error('No access'), { status: 403 }) })
  await recover(denied.r, denied.state, 0, () => null)
  assert.deepEqual(denied.state.failed, { reason: 'host', detail: 'No access' })
  assert.equal(denied.state.attempts, 0)
  const gone = harness(async () => {}, async () => { throw new Unreachable() })
  await recover(gone.r, gone.state, 0, () => null)
  assert.deepEqual(gone.state.failed, { reason: 'unreachable', detail: null }, 'a tunnel that never comes back ends in the error with Try again')
  assert.equal(gone.state.attempts, 0, 'no attempt was spent')
  assert.deepEqual(gone.log, ['sleep 0'])
})

test('closing the viewer mid-recovery stops it without an error', async () => {
  const { r, state, log } = harness(async () => { state.open = false; throw new Error('late') })
  await recover(r, state, 0, () => null)
  assert.equal(state.failed, null)
  assert.equal(log.filter((l) => l === 'reconnect').length, 1)
})

test('waits grow with each attempt and stop at ten seconds', () => {
  assert.deepEqual([0, 1, 2, 3, 5, 9].map(retryDelay), [2_000, 2_000, 4_000, 6_000, 10_000, 10_000])
})

test('a reconnect that answers but never goes live starts the next attempt, and fails only when the attempts are spent', async () => {
  const log: string[] = []
  const state = { attempts: 0, failed: null as Failure | null, open: true }
  const r: Recovery = {
    active: () => state.failed === null,
    route: async () => {},
    reconnect: async () => { log.push('reconnect') },
    sleep: async () => {},
    fail: (failure) => { state.failed = failure },
  }
  // The session's wiring: every reconnect answers, then the connect timer fires because ICE never connects.
  let again = true
  let rounds = 0
  while (again && rounds++ < 10) {
    again = false
    await recover(r, state, 0, () => null)
    if (state.failed === null) connectTimedOut(true, { retry: () => { again = true }, fail: (failure) => { state.failed = failure } })
  }
  assert.equal(log.filter((l) => l === 'reconnect').length, maxRecoveryAttempts)
  assert.equal(rounds, maxRecoveryAttempts + 1, 'three hung attempts, then the exhausted one fails')
  assert.deepEqual(state.failed, { reason: 'reconnect', detail: null })
})

test('the first connection that never goes live fails at once; a reconnect never fails on the timeout itself', () => {
  const calls: string[] = []
  const r = { retry: () => { calls.push('retry') }, fail: (f: Failure) => { calls.push(`fail ${f.reason}`) } }
  connectTimedOut(false, r)
  connectTimedOut(true, r)
  assert.deepEqual(calls, ['fail connect', 'retry'])
})
