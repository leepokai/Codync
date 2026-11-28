import assert from 'node:assert/strict'
import { test } from 'node:test'
import { accountComputerAction } from './account-computer-action.ts'

const studio = { computerId: 'studio', name: 'Studio', signKey: 'key', online: true, access: 'granted' }

test('an approved computer offers Open only once its bots have loaded', () => {
  const store = { isLoopback: false, roster: [] as unknown[] }
  assert.equal(accountComputerAction(studio, false, store, false), 'opening')
  store.roster.push({ id: 'lead' })
  assert.equal(accountComputerAction(studio, false, store, false), 'open')
})

test('a computer not reached yet offers to ask for access, or to cancel the request under way', () => {
  const other = { ...studio, access: 'none' }
  assert.equal(accountComputerAction(other, false, null, false), 'ask')
  assert.equal(accountComputerAction(other, false, null, true), 'cancel')
  assert.equal(accountComputerAction(other, true, null, false), null)
  assert.equal(accountComputerAction({ ...other, online: false }, false, null, false), null)
})
