import assert from 'node:assert/strict'
import { test } from 'node:test'
import { showsWelcome } from './welcome-gate.ts'

test('with accounts, the welcome covers the window whenever no one is signed in', () => {
  const signedOut = { onboarded: true, configured: true, ready: true, signedIn: false }
  assert.equal(showsWelcome(signedOut), true, 'signing out comes back to it')
  assert.equal(showsWelcome({ ...signedOut, signedIn: true }), false)
  assert.equal(showsWelcome({ ...signedOut, ready: false }), false, 'a saved session still restoring keeps the app')
  assert.equal(showsWelcome({ ...signedOut, signedIn: true, onboarded: false }), true, 'the first run still chooses')
})

test('without accounts, the first choice is the only welcome', () => {
  const local = { onboarded: false, configured: false, ready: true, signedIn: false }
  assert.equal(showsWelcome(local), true)
  assert.equal(showsWelcome({ ...local, onboarded: true }), false)
})
