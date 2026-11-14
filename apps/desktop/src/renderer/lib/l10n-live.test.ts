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

test('the update card names its action in the saved language', () => {
  // The button carries data (`Update now`), translated where the card draws it; the phone apps read
  // these same catalog files, so one entry reaches all three.
  lang = 'zh-Hans'
  assert.equal(trNow('Update now'), '立即更新')
  assert.equal(trNow('Update Codync'), '更新 Codync')
  lang = 'ru'
  assert.equal(trNow('Update now'), 'Обновить сейчас')
  lang = 'en'
  assert.equal(trNow('Update now'), 'Update now')
})
