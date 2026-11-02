import assert from 'node:assert/strict'
import { test } from 'node:test'
import { devicesLoseAccess, pairingAskText, spacedCode } from './account-copy.ts'

const tr = (text: string) => text

test('the pairing ask names the phone, its account and the spaced code', () => {
  assert.deepEqual(pairingAskText(tr, { deviceName: 'Pixel', email: 'k@example.com', code: '123456' }), {
    title: 'Let Pixel in?',
    message: "k@example.com wants to use this computer, which then joins k@example.com's account. Check Pixel shows 123\u00a0456.",
  })
})

test('a phone without an email is a Codync account, and names keep their braces', () => {
  const text = pairingAskText(tr, { deviceName: '{code}', email: null, code: '000042' })
  assert.equal(text.title, 'Let {code} in?')
  assert.equal(text.message, "A Codync account wants to use this computer, which then joins A Codync account's account. Check {code} shows 000\u00a0042.")
})

test('changing account drops every device of the old one', () => {
  assert.equal(devicesLoseAccess(tr, 'k@example.com'), "Every device of k@example.com's account loses access.")
  assert.equal(devicesLoseAccess(tr, null), 'Every device of that account loses access.')
})

test('codes other than 6 digits stay as they are', () => {
  assert.equal(spacedCode('12345'), '12345')
})
