import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Bot } from '../../../shared/models'
import { routineOwners, routineSections } from './all-routines.ts'
import type { Routine } from './routine'

const bot = (id: string, kind = 'agent') => ({ id, kind, name: id }) as Bot
const routine = (id: string, botId: string, name: string) =>
  ({ id, botId, name, instruction: '', triggers: [], enabled: true, createdAt: 0, updatedAt: 0, triggerDescriptions: [] }) as Routine

test('routines group under their bot in roster order, by name, and unknown bots stay out', () => {
  const sections = routineSections(
    [routine('1', 'b', 'Zeta'), routine('2', 'a', 'Daily'), routine('3', 'b', 'Alpha'), routine('4', 'gone', 'Lost')],
    [bot('b'), bot('a'), bot('c')],
  )
  assert.deepEqual(sections.map((s) => [s.bot.id, s.routines.map((r) => r.name)]), [['b', ['Alpha', 'Zeta']], ['a', ['Daily']]])
})

test('only agents can own a new routine', () => {
  assert.deepEqual(routineOwners([bot('a'), bot('g', 'group')]).map((b) => b.id), ['a'])
})
