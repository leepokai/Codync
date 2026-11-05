import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { ScreenState } from '../../../shared/models.ts'
import { accessGuidance, failureText, screenNotice } from './screen-guidance.ts'
import { offersRemoteScreen, screenAvailable } from './screen-access.ts'

const state = (extra: Partial<ScreenState>): ScreenState =>
  ({ enabled: true, connected: true, platform: 'macos', capture: true, input: true, displays: [], userControl: false, computerUse: true, viewers: 0, ...extra })
const same = (text: string) => text

test('each missing piece names what to do on the computer', () => {
  assert.match(accessGuidance(state({ enabled: false }), same), /^Remote screen is off\. On the computer, open Codync/)
  assert.match(accessGuidance(state({ connected: false }), same), /^Codync Screen is not running\./)
  assert.match(accessGuidance(state({ capture: false, permissionApp: 'Codync Dev' }), same), /^Allow screen recording for Codync Dev\./)
  assert.match(accessGuidance(state({ capture: false }), same), /^Allow screen recording for Codync Screen\./)
  assert.match(accessGuidance(state({ input: false }), same), /^Allow computer control for Codync Screen\. macOS may call this/)
  assert.match(accessGuidance(state({ platform: 'linux', input: false }), same), /^Choose a screen and allow control/)
  assert.match(accessGuidance(state({ platform: 'windows' }), same), /not available on Windows/)
  assert.equal(accessGuidance(state({}), same), 'Computer access is ready.')
})

test('screen is available once enabled, running and allowed to record', () => {
  assert.equal(screenAvailable(null), false)
  assert.equal(screenAvailable(state({})), true)
  assert.equal(screenAvailable(state({ input: false })), true, 'view only still views')
  for (const off of [{ enabled: false }, { connected: false }, { capture: false }]) assert.equal(screenAvailable(state(off)), false)
})

test('the button shows for another computer that is online and viewable, never this one', () => {
  assert.equal(offersRemoteScreen(state({}), true, false), true)
  assert.equal(offersRemoteScreen(state({}), true, true), false, 'this computer')
  assert.equal(offersRemoteScreen(state({}), false, false), false, 'offline')
  assert.equal(offersRemoteScreen(state({ capture: false }), true, false), false, 'no screen recording')
  assert.equal(offersRemoteScreen(null, true, false), false, 'no screen state yet')
})

test('a failure reads in this app\'s wording, except the host\'s own text, which is shown as it wrote it', () => {
  const tr = (text: string) => `«${text}»`
  assert.equal(failureText({ reason: 'connect', detail: null }, tr), "«Couldn't connect to the remote screen. Check the computer's connection and try again.»")
  assert.equal(failureText({ reason: 'reconnect', detail: null }, tr), "«Couldn't reconnect to the remote screen.»")
  assert.equal(failureText({ reason: 'unreachable', detail: null }, tr), "«Couldn't reach the computer.»")
  assert.equal(failureText({ reason: 'host', detail: 'Remote screen is turned off on this computer.' }, tr), 'Remote screen is turned off on this computer.')
})

test('the notice names why the screen is missing, and nothing while the video plays', () => {
  const base = { live: false, unreachable: false, screen: null, phase: undefined }
  assert.deepEqual(screenNotice({ ...base, unreachable: true }, same), { icon: 'wifi.slash', text: 'This computer is offline.' })
  assert.deepEqual(screenNotice(base, same), { text: 'Connecting to the screen…' })
  assert.match(screenNotice({ ...base, screen: state({ enabled: false }) }, same)?.text ?? '', /^Remote screen is off\./)
  const live = { ...base, live: true, screen: state({}) }
  assert.deepEqual(screenNotice({ ...live, phase: { kind: 'connecting' } }, same), { text: 'Connecting to the screen…' })
  assert.deepEqual(screenNotice({ ...live, phase: { kind: 'reconnecting' } }, same), { text: 'Reconnecting…', dim: true })
  const failed = screenNotice({ ...live, phase: { kind: 'failed', failure: { reason: 'unreachable', detail: null } } }, same)
  assert.deepEqual(failed, { icon: 'exclamationmark.triangle', text: "Couldn't reach the computer.", retry: true })
  assert.equal(screenNotice({ ...live, phase: { kind: 'live' } }, same), null)
})
