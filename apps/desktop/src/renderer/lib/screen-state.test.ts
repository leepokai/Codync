import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { ScreenState } from '@shared/models'
import { mainDisplay, screenAvailable, screenGuidance } from './screen-state.ts'

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
})

test('guidance names what is missing, in the phrases the apps already translate', () => {
  assert.equal(screenGuidance(ready, same), 'Computer access is ready.')
  assert.match(screenGuidance({ ...ready, enabled: false }, same), /^Remote screen is off\. On the computer/)
  assert.match(screenGuidance({ ...ready, input: false }, same), /^Choose a screen and allow control/)
  assert.match(screenGuidance({ ...ready, platform: 'macos', input: false }, same), /^Allow computer control for Codync Screen\./)
  assert.match(screenGuidance({ ...ready, platform: 'windows' }, same), /not available on Windows yet/)
})
