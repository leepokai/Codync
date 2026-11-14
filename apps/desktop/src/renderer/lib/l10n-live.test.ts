import assert from 'node:assert/strict'
import { test } from 'node:test'
import { trNow } from './l10n.ts'
import type { RoutineRun } from '../views/routines/routine.ts'

// trNow reads the language saved in Settings on every call; the stub below stands in for localStorage.
let lang = 'ru'
Object.defineProperty(globalThis, 'localStorage', {
  value: { getItem: (key: string) => (key === 'appLanguage' ? JSON.stringify(lang) : null) },
  configurable: true,
  writable: true,
})

test('static labels stay English data and translate when shown, not when the module loads', async () => {
  // Loaded only now, while the saved language is Russian: a label translated at import would stay Russian.
  const { repeatChoices, runLabel } = await import('../views/routines/routine.ts')

  const daily = repeatChoices.find((c) => c.id === 'daily')
  assert.equal(daily?.label, 'Daily')
  assert.equal(trNow(daily!.label), 'Каждый день')
  assert.equal(runLabel({ id: 'r', routineId: 'x', botId: 'b', status: 'running', createdAt: 0, notBefore: null } as RoutineRun), 'Выполняется')

  lang = 'en'
  assert.equal(trNow(daily!.label), 'Daily')
  assert.equal(runLabel({ id: 'r', routineId: 'x', botId: 'b', status: 'running', createdAt: 0, notBefore: null } as RoutineRun), 'Running')
})
