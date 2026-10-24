import assert from 'node:assert/strict'
import { test } from 'node:test'
import { normalizeBot } from '../../../shared/models.ts'
import { afterExit } from '../../lib/exit-delay.ts'
import { workingExitMs, workingTransition } from './working-transition.ts'

const working = (startedAt: number) => normalizeBot({ id: 'bot', name: 'Bot', startedAt, status: 'working' })
const finished = normalizeBot({ id: 'bot', name: 'Bot', status: 'idle' })

test('finished activity keeps its last working bot for 420 ms, then leaves', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const live = working(100)
  let mounted = true
  const cancel = afterExit(workingExitMs, () => { mounted = false })
  const shown = () => workingTransition(finished, live, false, mounted).indicator
  assert.equal(shown(), live)
  t.mock.timers.tick(419)
  assert.equal(shown()?.startedAt, 100)
  t.mock.timers.tick(1)
  assert.equal(shown(), null)
  cancel()
})

test('restarting during exit cancels removal and retains the new activity on its next exit', (t) => {
  t.mock.timers.enable({ apis: ['setTimeout'] })
  const previous = working(100)
  let mounted = true
  const cancel = afterExit(workingExitMs, () => { mounted = false })
  t.mock.timers.tick(200)
  assert.equal(workingTransition(finished, previous, false, mounted).indicator, previous)
  cancel() // usePresence effect cleanup when visible becomes true again.
  const restarted = working(200)
  const next = workingTransition(restarted, previous, true, mounted)
  assert.equal(next.indicator, restarted)
  t.mock.timers.tick(1000)
  assert.equal(mounted, true)
  assert.equal(workingTransition(finished, next.active, false, mounted).indicator, restarted)
})
