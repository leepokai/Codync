import assert from 'node:assert/strict'
import test from 'node:test'
import { relayAddressToSave } from './relay-address.ts'

test('a typed relay address is stored only when it changes', () => {
  assert.equal(relayAddressToSave('  https://relay.example.com/  ', 'https://relay.example.com'), null)
  assert.equal(relayAddressToSave('https://relay.example.com///', 'https://relay.example.com'), null)
  assert.equal(relayAddressToSave('   ', null), null)
  assert.equal(relayAddressToSave('https://other.example', 'https://relay.example.com'), 'https://other.example')
  assert.equal(relayAddressToSave('  ', 'https://relay.example.com'), '')
})
