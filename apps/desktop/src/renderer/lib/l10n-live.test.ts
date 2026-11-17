import assert from 'node:assert/strict'
import { test } from 'node:test'
import { trNow } from './l10n.ts'
import { fill } from '../../shared/fill.ts'
import { stepBannerPhrase } from '../../shared/fallbacks.ts'
import { normalizeBot } from '../../shared/models.ts'
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

test('the Agent card names the fallback banner in the saved language', () => {
  lang = 'zh'
  assert.equal(trNow('Using Codex until Claude Code resets at 3:00 PM'), '正在使用 Codex，等 Claude Code 在 3:00 PM 重置后切回')
  assert.equal(trNow('Using Codex while Claude Code is signed out'), '正在使用 Codex，Claude Code 已退出登录')
  assert.equal(trNow("Using Codex while Claude Code can't answer"), '正在使用 Codex，Claude Code 暂时无法回答')
  lang = 'de'
  assert.equal(trNow('Using Codex while Claude Code is rate limited'), 'Codex wird verwendet, da Claude Code rate-limitiert ist')
  lang = 'en'
  assert.equal(trNow('Using Codex while Claude Code is signed out'), 'Using Codex while Claude Code is signed out')
})

test('the banner is translated as a phrase, so a name holding " while " stays whole', () => {
  lang = 'zh'
  const phrase = 'Using {to} while {own} is signed out'
  // "Work while traveling" is a login profile's name; the phrase is translated before it goes in.
  assert.equal(fill(trNow(phrase), { to: 'Codex', own: 'Claude Code (Work while traveling)' }), '正在使用 Codex，Claude Code (Work while traveling) 已退出登录')
  lang = 'en'
  assert.equal(fill(trNow(phrase), { to: 'Codex', own: 'Claude Code (Work while traveling)' }), 'Using Codex while Claude Code (Work while traveling) is signed out')
})

test('every banner the host can cause has its phrase in the catalogs', () => {
  lang = 'zh-Hant'
  const step = { step: 1, agent: 'codex', model: null, since: 0, reason: 'signedOut' }
  const banner = (reason: string | null) => stepBannerPhrase(normalizeBot({ id: 'b', backend: 'claude', agentStep: step,
    resting: reason ? [{ agent: 'claude', until: Date.now() + 3_600_000, reason }] : [] }), (l) => l.backend, Date.now())!.key
  // The reasons `rest::record` rests an agent for; a missing model or mode never rests one.
  for (const reason of [null, 'usageLimit', 'outOfCredits', 'rateLimited', 'overloaded', 'signedOut', 'notStarting']) {
    assert.notEqual(trNow(banner(reason)), banner(reason), String(reason))
  }
  assert.equal(fill(trNow(banner('signedOut')), { to: 'Codex', own: 'Kimi Code' }), '正在使用 Codex，Kimi Code 已登出')
})

test("a resting agent's line follows the saved language", () => {
  lang = 'zh'
  assert.equal(trNow('Usage resets 3:00 PM'), '用量将在 3:00 PM 重置')
  assert.equal(trNow('Resting until 3:00 PM'), '休息到 3:00 PM')
  lang = 'de'
  assert.equal(trNow('Usage resets 3:00 PM'), 'Nutzung wird um 3:00 PM zurückgesetzt')
  lang = 'en'
  assert.equal(trNow('Resting until 3:00 PM'), 'Resting until 3:00 PM')
})
