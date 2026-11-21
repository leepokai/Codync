import assert from 'node:assert/strict'
import { test } from 'node:test'
import { clockTime, freeLogins, mixesAgents, moveFallback, restLine, restOf, restPhrase, runningAgent, sameLogin, stepAgents, stepBannerPhrase, stepLogins, stepOn, withAdvisor, withBackend, withThreadBackend } from './fallbacks.ts'
import { fill } from './fill.ts'
import { draftOf, normalizeBot, type Backend, type Bot } from './models.ts'

const now = new Date(2026, 9, 21, 10, 0).getTime()
const at3 = new Date(2026, 9, 21, 15, 0).getTime()
const time = (ms: number) => new Date(ms).toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' })
const names: Record<string, string> = { claude: 'Claude Code', codex: 'Codex' }
const name = (id: string) => names[id] ?? id
/** The banner as English reads it: the phrase with its names put in. */
const stepBanner = (bot: Bot, called: Parameters<typeof stepBannerPhrase>[1], at: number) => {
  const phrase = stepBannerPhrase(bot, called, at)
  return phrase && fill(phrase.key, phrase.values)
}

test('a time today is just the time; another day names the weekday', () => {
  assert.equal(clockTime(at3, now), time(at3))
  const friday = new Date(2026, 9, 23, 15, 0).getTime()
  assert.equal(clockTime(friday, now), `${new Date(friday).toLocaleDateString(undefined, { weekday: 'short' })} ${time(friday)}`)
})

test('a usage limit says when it resets; other rests say until when', () => {
  assert.equal(restLine({ agent: 'claude', until: at3, reason: 'usageLimit' }, now), `Usage resets ${time(at3)}`)
  assert.equal(restLine({ agent: 'claude', until: at3, reason: 'rateLimited' }, now), `Resting until ${time(at3)}`)
})

test('a rest that ended no longer counts', () => {
  const bot = normalizeBot({ id: 'b', resting: [{ agent: 'claude', until: now - 1, reason: 'usageLimit' }] })
  assert.equal(restOf(bot, { backend: 'claude' }, now), null)
  assert.equal(restOf(bot, { backend: 'claude' }, now - 2)?.reason, 'usageLimit')
})

test('an agent comes again in the chain only on another of its login profiles', () => {
  const claude: Backend = { id: 'claude', name: 'Claude Code', available: true, command: '', installHint: '',
    loginProfiles: [{ id: 'a1', kind: 'account', name: 'Work', signedIn: true }, { id: 'a2', kind: 'apiKey', name: null, signedIn: false }] }
  assert.deepEqual(freeLogins(claude, []), [null, 'a1'], "one that isn't signed in can't carry a turn")
  assert.deepEqual(freeLogins(claude, [{ backend: 'claude' }]), ['a1'], "the bot's own agent, on the computer's login")
  assert.deepEqual(freeLogins(claude, [{ backend: 'claude', loginProfile: 'a1' }, { backend: 'codex' }]), [null])
  assert.deepEqual(freeLogins({ ...claude, loginProfiles: undefined }, [{ backend: 'claude' }]), [], 'an agent with one login')
  // A rest is one login's: the login profile's leaves the computer's own free.
  const bot = normalizeBot({ id: 'b', resting: [{ agent: 'claude', loginProfile: 'a1', until: now + 1, reason: 'usageLimit' }] })
  assert.equal(restOf(bot, { backend: 'claude' }, now), null)
  assert.equal(restOf(bot, { backend: 'claude', loginProfile: 'a1' }, now)?.reason, 'usageLimit')
  assert.equal(sameLogin({ backend: 'claude', loginProfile: null }, { backend: 'claude' }), true)
})

test('the banner names the fallback, and when the own agent is back if it says', () => {
  const step = { step: 1, agent: 'codex', model: null, since: now, reason: 'usageLimit' }
  // "Claude Code (Work)" on a login profile; the computer's own login by name too when it's asked for.
  const called = (login: { backend: string; loginProfile?: string | null }, own: boolean) =>
    `${name(login.backend)}${login.loginProfile ? ' (Work)' : own ? ' (Default login profile)' : ''}`
  assert.equal(stepBanner(normalizeBot({ id: 'b', backend: 'claude' }), called, now), null)
  const resting = normalizeBot({ id: 'b', backend: 'claude', agentStep: step, resting: [{ agent: 'claude', until: at3, reason: 'usageLimit' }] })
  assert.equal(stepBanner(resting, called, now), `Using Codex until Claude Code resets at ${time(at3)}`)
  const failing = normalizeBot({ id: 'b', backend: 'claude', agentStep: step, resting: [{ agent: 'claude', until: now - 1, reason: 'usageLimit' }] })
  assert.equal(stepBanner(failing, called, now), "Using Codex while Claude Code can't answer")
  // Two logins of one agent go by their login profiles.
  const onProfile = normalizeBot({ id: 'b', backend: 'claude', agentStep: { ...step, agent: 'claude', loginProfile: 'a1' } })
  assert.equal(stepBanner(onProfile, called, now), "Using Claude Code (Work) while Claude Code (Default login profile) can't answer")
})

test('only a usage limit gives the banner a reset time; other rests name their reason (#444)', () => {
  const step = { step: 1, agent: 'codex', model: null, since: now, reason: 'signedOut' }
  const called = (login: { backend: string }) => name(login.backend)
  const resting = (reason: string) =>
    normalizeBot({ id: 'b', backend: 'claude', agentStep: step, resting: [{ agent: 'claude', until: at3, reason }] })
  assert.equal(stepBanner(resting('usageLimit'), called, now), `Using Codex until Claude Code resets at ${time(at3)}`)
  for (const reason of ['signedOut', 'rateLimited', 'outOfCredits', 'overloaded', 'notStarting']) {
    assert.doesNotMatch(stepBanner(resting(reason), called, now) ?? '', /resets/, reason)
  }
  assert.equal(stepBanner(resting('signedOut'), called, now), 'Using Codex while Claude Code is signed out')
  assert.equal(restPhrase('somethingElse'), "can't answer")
})

test('a login profile name holding " while " stays one name, not the banner\'s own word', () => {
  const step = { step: 1, agent: 'codex', model: null, since: now, reason: 'signedOut' }
  const called = (login: { backend: string; loginProfile?: string | null }, own: boolean) =>
    `${name(login.backend)}${login.loginProfile ? ' (Work while traveling)' : own ? ' (Default login profile)' : ''}`
  const signingOut = normalizeBot({ id: 'b', backend: 'claude', loginProfile: 'p1', agentStep: step, resting: [{ agent: 'claude', loginProfile: 'p1', until: at3, reason: 'signedOut' }] })
  const phrase = stepBannerPhrase(signingOut, called, now)
  // The phrase keeps its slot, and the whole name is one value: translating the phrase never sees the name.
  assert.equal(phrase?.key, 'Using {to} while {own} is signed out')
  assert.deepEqual(phrase?.values, { to: 'Codex', own: 'Claude Code (Work while traveling)' })
  assert.equal(fill(phrase!.key, phrase!.values), 'Using Codex while Claude Code (Work while traveling) is signed out')
})

test('fallbacks move within the list', () => {
  const f = (backend: string) => ({ backend, model: null, modelOptions: {} })
  const list = [f('codex'), f('gemini'), f('grok')]
  assert.deepEqual(moveFallback(list, 0, 1).map((x) => x.backend), ['gemini', 'codex', 'grok'])
  assert.deepEqual(moveFallback(list, 2, -1).map((x) => x.backend), ['codex', 'grok', 'gemini'])
  assert.deepEqual(moveFallback(list, 0, -1).map((x) => x.backend), ['codex', 'gemini', 'grok'])
  assert.deepEqual(list.map((x) => x.backend), ['codex', 'gemini', 'grok'])
})

test('making a fallback the bot\'s own agent drops it from the list', () => {
  const draft = draftOf(normalizeBot({ id: 'b', backend: 'claude', fallbacks: [{ backend: 'codex', model: 'gpt-5.5', modelOptions: {} }, { backend: 'gemini', model: null, modelOptions: {} }] }))
  const next = withBackend(draft, 'codex')
  assert.equal(next.backend, 'codex')
  assert.deepEqual(next.fallbacks?.map((f) => f.backend), ['gemini'])
})

test('Advisor mode starts the threads on the bot\'s own agent and chain, and another threads\' agent drops its login from theirs', () => {
  const codex = { backend: 'codex', model: null, modelOptions: {} }
  const draft = draftOf(normalizeBot({ id: 'b', backend: 'claude', model: 'opus', fallbacks: [codex], threadAgent: null, threadFallbacks: [] }))
  assert.equal(draft.threadAgent, null, 'a host with Advisor mode: off is sent as null')
  const on = withAdvisor(draft, true, [])
  assert.deepEqual(on.threadAgent, { backend: 'claude', loginProfile: null, model: 'opus', modelOptions: {} })
  assert.deepEqual(on.threadFallbacks, [codex])
  const moved = withThreadBackend(on, 'codex')
  assert.deepEqual([moved.threadAgent?.backend, moved.threadAgent?.model, moved.threadFallbacks], ['codex', null, []])
  assert.equal(withThreadBackend(moved, 'codex'), moved)
  assert.deepEqual(moved.fallbacks, [codex], 'the chat\'s chain is its own')
  assert.deepEqual([withAdvisor(moved, false, []).threadAgent, withAdvisor(moved, false, []).threadFallbacks], [null, []])
  const installed = { id: 'gemini', name: 'Gemini', available: true } as Backend
  assert.equal(withAdvisor({ ...draft, backend: 'custom' }, true, [installed]).threadAgent?.backend, 'gemini', 'the picker has no custom command')
})

test('an older host has no fallbacks to show or send', () => {
  const bot = normalizeBot({ id: 'b' })
  assert.equal(bot.selfFallbacks, null)
  assert.equal(draftOf(bot).selfFallbacks, undefined)
  assert.equal(bot.threadFallbacks, null)
  assert.equal('threadAgent' in draftOf(bot), false, 'nor Advisor mode')
  assert.equal(normalizeBot({ id: 'b', selfFallbacks: false }).selfFallbacks, false)
})


test('switching harness clears its model and effort, selecting the same harness preserves them', () => {
  const draft = draftOf(normalizeBot({ id: 'b', backend: 'claude', loginProfile: 'a1', model: 'opus', modelOptions: { effort: 'high' } }))
  assert.equal(withBackend(draft, 'claude'), draft)
  const changed = withBackend(draft, 'codex')
  assert.equal(changed.model, null)
  assert.equal(changed.loginProfile, null, 'a login profile is its own agent\'s')
  assert.deepEqual(changed.modelOptions, {})
  const defaultModel = withBackend({ ...draft, model: null }, 'codex')
  assert.deepEqual(defaultModel.modelOptions, {})
})

test('bots mix agents when they run more than one, counting a fallback and leaving groups out', () => {
  const claude = normalizeBot({ id: 'a', backend: 'claude' })
  const onCodex = normalizeBot({ id: 'b', backend: 'claude', agentStep: { step: 1, agent: 'codex', model: null, since: now, reason: 'usageLimit' } })
  const group = normalizeBot({ id: 'g', kind: 'group', backend: 'codex' })
  assert.equal(runningAgent(onCodex), 'codex')
  assert.equal(mixesAgents([claude, normalizeBot({ id: 'c', backend: 'claude' }), group]), false)
  assert.equal(mixesAgents([claude, onCodex]), true)
})

test('a step picked in place takes a free login, and gets its own back on its own agent', () => {
  const claude: Backend = { id: 'claude', name: 'Claude Code', available: true, command: '', installHint: '',
    loginProfiles: [{ id: 'a1', kind: 'account', name: 'Work', signedIn: true }, { id: 'a2', kind: 'account', name: 'Home', signedIn: false }] }
  const codex: Backend = { id: 'codex', name: 'Codex', available: true, command: '', installHint: '' }
  const taken = [{ backend: 'claude' }, { backend: 'codex' }]
  assert.deepEqual(stepOn(claude, taken), { backend: 'claude', loginProfile: 'a1', model: null, modelOptions: {} })
  const own = { backend: 'claude', loginProfile: 'a2', model: 'opus', modelOptions: { effort: 'high' } }
  assert.equal(stepOn(claude, taken, own), own, 'back on its own agent, its model and login come back')
  assert.equal(stepOn(claude, [...taken, { backend: 'claude', loginProfile: 'a2' }], own).loginProfile, 'a1', 'unless another step has its login now')
  // Its signed-out login stays a choice, and so does its agent.
  assert.deepEqual(stepLogins(claude, own, taken), ['a1', 'a2'])
  assert.deepEqual(stepAgents([claude, codex], [{ backend: 'claude' }, { backend: 'claude', loginProfile: 'a1' }, { backend: 'codex' }]), [])
  assert.deepEqual(stepAgents([claude, codex], [{ backend: 'claude' }, { backend: 'claude', loginProfile: 'a1' }, { backend: 'codex' }], [own]), [claude])
  // Once another step has the remembered login and no other is free, its agent can't be picked: it would land on the bot's own.
  assert.deepEqual(stepAgents([claude, codex], [{ backend: 'claude' }, { backend: 'claude', loginProfile: 'a1' }, { backend: 'codex' }, { backend: 'claude', loginProfile: 'a2' }], [own]), [])
})
