import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { ScreenState } from '@shared/models'
import type { ScreenPhase } from '../store/screen-session.ts'
import { SCREEN_IDLE_MS, controlReady, displayToShow, mainDisplay, untouchedTooLong, screenAvailable, screenGuidance, screenNotice } from './screen-state.ts'

const ready: ScreenState = {
  enabled: true, connected: true, platform: 'linux', capture: true, input: true, userControl: false, agentBot: null, computerUse: true, viewers: 0,
  displays: [{ id: 0, name: 'Screen', width: 1280, height: 800, main: false }, { id: 1, name: 'Main', width: 1920, height: 1080, main: true }],
}
const same = (text: string) => text

test('a screen is available once it is on, running and allowed to record', () => {
  assert.equal(screenAvailable(ready), true)
  assert.equal(screenAvailable({ ...ready, connected: false }), false)
  assert.equal(screenAvailable(null), false)
  assert.equal(mainDisplay(ready)?.id, 1)
  assert.equal(displayToShow(ready, 0)?.id, 0, 'the display picked before')
  assert.equal(displayToShow(ready, 7)?.id, 1, 'gone since: the main one')
  assert.equal(displayToShow(ready, null)?.id, 1)
})

test('bots can use the computer once control is allowed too, or when Windows says so', () => {
  assert.equal(controlReady(ready), true)
  assert.equal(controlReady({ ...ready, input: false }), false)
  assert.equal(controlReady({ ...ready, platform: 'windows', enabled: false, computerUse: true }), true)
  assert.equal(controlReady({ ...ready, platform: 'windows', computerUse: false }), false)
})

test('guidance names what is missing, in the phrases the apps already translate', () => {
  assert.equal(screenGuidance(ready, same), 'Computer access is ready.')
  assert.match(screenGuidance({ ...ready, enabled: false }, same), /^Remote screen is off\. On the computer/)
  assert.match(screenGuidance({ ...ready, input: false }, same), /^Choose a screen and allow control/)
  assert.match(screenGuidance({ ...ready, platform: 'macos', input: false }, same), /^Allow computer control for Codync Screen\./)
  assert.match(screenGuidance({ ...ready, platform: 'windows' }, same), /not available on Windows yet/)
})

test('the picture says what keeps it away, a sleeping cloud computer first', () => {
  const awake = { screen: ready, asleep: false, connection: 'online' } as const
  assert.deepEqual(screenNotice({ ...awake, screen: null, asleep: true, connection: 'offline' }, null, false, same), { text: 'Waking the computer…' })
  assert.deepEqual(screenNotice({ ...awake, connection: 'connecting' }, null, false, same), { text: 'Connecting to the screen…' })
  assert.equal(screenNotice({ ...awake, connection: 'offline' }, null, false, same)?.icon, 'wifi.slash')
  assert.equal(screenNotice({ ...awake, screen: null }, null, false, same)?.icon, 'arrow.down.circle')
  assert.equal(screenNotice({ ...awake, screen: { ...ready, enabled: false } }, null, false, same)?.icon, 'lock.display')
  const session = (phase: ScreenPhase) => ({ phase, start: async () => {} })
  assert.deepEqual(screenNotice(awake, null, true, same), { text: 'Connecting to the screen…' })
  assert.deepEqual(screenNotice(awake, session({ kind: 'reconnecting' }), true, same), { text: 'Reconnecting…' })
  assert.equal(screenNotice(awake, session({ kind: 'failed', message: 'No route' }), true, same)?.text, 'No route')
  assert.equal(screenNotice(awake, session({ kind: 'live' }), true, same), null)
})

test('a screen pauses once nobody has touched the app for the idle time', () => {
  assert.equal(untouchedTooLong(1_000, 1_000 + SCREEN_IDLE_MS - 1), false)
  assert.equal(untouchedTooLong(1_000, 1_000 + SCREEN_IDLE_MS), true)
})
