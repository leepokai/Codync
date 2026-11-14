import assert from 'node:assert/strict'
import { test } from 'node:test'
import { AuthenticationEnded, BrowserAuthentication } from '../main/browser-authentication.ts'

const turn = () => new Promise<void>((resolve) => setImmediate(resolve))

test('a stale same-scheme callback leaves the current browser login waiting', async () => {
  const browser = new BrowserAuthentication(async () => {})
  const first = browser.authenticate('https://example.test/first', 'codync')
  const firstEnded = assert.rejects(first, (e: unknown) => e instanceof AuthenticationEnded && e.reason === 'cancelled')
  let resolved = false
  const second = browser.authenticate('https://example.test/second', 'codync', async (url) => url.endsWith('current'))
  void second.then(() => { resolved = true })
  await firstEnded
  browser.handleURL('codync://stale')
  await turn()
  assert.equal(resolved, false)
  browser.handleURL('other://current')
  await turn()
  assert.equal(resolved, false)
  browser.handleURL('codync://current')
  assert.equal(await second, 'codync://current')
})

test('browser-open and callback-validation failures end their waits', async () => {
  const cannotOpen = new BrowserAuthentication(async () => { throw new Error('No browser') })
  await assert.rejects(cannotOpen.authenticate('https://example.test', 'codync'), /No browser/)
  const browser = new BrowserAuthentication(async () => {})
  const result = browser.authenticate('https://example.test', 'codync', async () => { throw new Error('Offline') })
  browser.handleURL('codync://callback')
  await assert.rejects(result, /Offline/)
})

test('replacing a wait clears its timeout and a later open failure cannot reject the replacement', async () => {
  let failOpen: (error: Error) => void = () => {}
  let calls = 0
  const browser = new BrowserAuthentication(async () => {
    if (++calls === 1) await new Promise<void>((_, reject) => { failOpen = reject })
  }, 15)
  const first = browser.authenticate('https://example.test/old', 'codync')
  const rejected = assert.rejects(first, AuthenticationEnded)
  const second = browser.authenticate('https://example.test/new', 'codync')
  await rejected
  failOpen(new Error('Old browser failed'))
  browser.handleURL('codync://callback')
  assert.equal(await second, 'codync://callback')
  const third = browser.authenticate('https://example.test/timeout', 'codync')
  await assert.rejects(third, (e: unknown) => e instanceof AuthenticationEnded && e.reason === 'timeout')
})
