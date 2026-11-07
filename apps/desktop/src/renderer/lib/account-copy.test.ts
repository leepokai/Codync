import assert from 'node:assert/strict'
import { test } from 'node:test'
import { devicesLoseAccess, joinRemovesText, leavingRemovesText, pairingAnswers, pairingAskText, spacedCode, withJoinRemoves } from './account-copy.ts'

const tr = (text: string) => text

test('a signed-in phone is asked about the account, and told what staying out means', () => {
  const ask = { deviceName: 'Pixel', email: 'k@example.com', code: '123456', canJoin: true, phoneOnly: true }
  assert.deepEqual(pairingAskText(tr, ask), {
    title: "Add this computer to k@example.com's account?",
    message: 'Without it, this phone can connect only over Wi-Fi or Tailscale. Check Pixel shows 123 456.',
  })
})

test('a phone that has to join is only asked about joining', () => {
  const ask = { deviceName: 'Pixel', email: 'k@example.com', code: '123456', canJoin: true, phoneOnly: false }
  assert.equal(pairingAskText(tr, ask).message, 'Pixel wants to add it. Check Pixel shows 123 456.')
})

test('a signed-out phone is let in or not, with no account in the question', () => {
  const ask = { deviceName: 'Pixel', code: '123456', canJoin: false, phoneOnly: true }
  assert.deepEqual(pairingAskText(tr, ask), {
    title: 'Let Pixel in?',
    message: 'It can connect only over Wi-Fi or Tailscale. Check Pixel shows 123 456.',
  })
})

test('a join names the phones it removes', () => {
  const ask = { deviceName: 'Pixel', email: 'k@example.com', code: '123456', canJoin: true, phoneOnly: true, removes: ['Old', 'iPad'] }
  assert.match(pairingAskText(tr, ask).message, /These phones lose access and must pair again: Old, iPad\./)
  assert.equal(joinRemovesText(tr, []), "")
  assert.equal(joinRemovesText(tr, null), "")
})

test('a host before these answers says plainly that letting the phone in joins its account', () => {
  const ask = { deviceName: 'Pixel', email: 'k@example.com', code: '123456' }
  assert.equal(pairingAskText(tr, ask).message, "k@example.com wants to use this computer, which then joins k@example.com's account. Check Pixel shows 123\u00a0456.")
  assert.deepEqual(pairingAnswers({ canJoin: null }), [{ title: 'Let in' }])
})

test('the answers offered follow what the host says the phone may get', () => {
  assert.deepEqual(pairingAnswers({ canJoin: true, phoneOnly: true }), [{ title: 'Wi-Fi or Tailscale only', join: false }, { title: 'Add to account', join: true }])
  assert.deepEqual(pairingAnswers({ canJoin: true, phoneOnly: false }), [{ title: 'Add to account', join: true }])
  assert.deepEqual(pairingAnswers({ canJoin: false, phoneOnly: true }), [{ title: 'Let in' }])
})

test('the phones a join removes follow what it says', () => {
  assert.equal(withJoinRemoves(tr, 'Add it?', []), 'Add it?')
  assert.equal(withJoinRemoves(tr, 'Add it?', ['Old']), 'Add it? These phones lose access and must pair again: Old.')
  assert.equal(withJoinRemoves(tr, 'Add it?', null), 'Add it? Phones paired without this account lose access and must pair again.')
})

test('a phone without an email is a Codync account, and names keep their braces', () => {
  const text = pairingAskText(tr, { deviceName: '{code}', email: null, code: '000042', canJoin: true, phoneOnly: true })
  assert.equal(text.title, "Add this computer to A Codync account's account?")
  assert.equal(text.message, 'Without it, this phone can connect only over Wi-Fi or Tailscale. Check {code} shows 000 042.')
})

test('changing account drops every device of the old one', () => {
  assert.equal(devicesLoseAccess(tr, 'k@example.com'), "Every device of k@example.com's account loses access.")
  assert.equal(devicesLoseAccess(tr, null), 'Every device of that account loses access.')
})

test('leaving an account names the devices it drops, when the computer could list them', () => {
  assert.equal(leavingRemovesText(tr, 'k@example.com', ['Pixel', 'iPad']), 'These phones lose access and must pair again: Pixel, iPad.')
  assert.equal(leavingRemovesText(tr, 'k@example.com', []), "Every device of k@example.com's account loses access.")
  assert.equal(leavingRemovesText(tr, null, null), 'Every device of that account loses access.')
})

test('codes other than 6 digits stay as they are', () => {
  assert.equal(spacedCode('12345'), '12345')
})
