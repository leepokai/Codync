import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { HostClient } from '../client/host-client'
import type { Routine } from '../views/routines/routine'
import { AllRoutinesStore } from './all-routines.ts'

test('turning a routine off asks the host, then lists again', async () => {
  const sent: unknown[][] = []
  let enabled = true
  const routine = { id: 'r1', botId: 'b1', name: 'Daily', enabled } as Routine
  const client = {
    call: async (method: string, body: unknown) => {
      sent.push([method, body])
      if (method === 'setRoutineEnabled') enabled = false
      return { routines: [{ ...routine, enabled }] }
    },
  } as unknown as HostClient
  const model = new AllRoutinesStore(async () => client)
  await model.load()
  assert.equal(model.loaded, true)
  await model.setEnabled(model.routines[0]!, false)
  assert.deepEqual(sent.map(([method]) => method), ['allRoutines', 'setRoutineEnabled', 'allRoutines'])
  assert.deepEqual(sent[1]![1], { botId: 'b1', id: 'r1', enabled: false })
  assert.equal(model.routines[0]!.enabled, false)
  assert.equal(model.busy, false)
})

test('a failed listing keeps the last routines and shows why', async () => {
  let fail = false
  const client = { call: async () => { if (fail) throw new Error('offline'); return { routines: [{ id: 'r1', botId: 'b1' }] } } } as unknown as HostClient
  const model = new AllRoutinesStore(async () => client)
  await model.load()
  fail = true
  await model.load()
  assert.equal(model.routines.length, 1)
  assert.equal(model.error, 'offline')
})
