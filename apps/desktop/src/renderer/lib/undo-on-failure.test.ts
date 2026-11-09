import assert from 'node:assert/strict'
import { test } from 'node:test'
import { undoOnFailure } from './undo-on-failure.ts'

/** What happens, in order, when `action` settles, then whether it went through. */
async function settle(action: Promise<unknown>, withUndo = true) {
  const log: string[] = []
  const accepted = await undoOnFailure(action, withUndo ? () => log.push('undo') : undefined, (message) => log.push(`error: ${message}`))
  return [...log, accepted ? 'accepted' : 'refused']
}

test('a refused action is taken back before its error shows', async () => {
  assert.deepEqual(await settle(Promise.reject(new Error('Section names are 40 characters at most'))), ['undo', 'error: Section names are 40 characters at most', 'refused'])
  assert.deepEqual(await settle(Promise.reject('timed out')), ['undo', 'error: timed out', 'refused'])
})

test('an accepted action keeps what it showed', async () => {
  assert.deepEqual(await settle(Promise.resolve()), ['accepted'])
})

test('an action with nothing to take back still shows its error', async () => {
  assert.deepEqual(await settle(Promise.reject(new Error('offline')), false), ['error: offline', 'refused'])
})
