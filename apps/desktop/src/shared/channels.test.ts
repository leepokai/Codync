import assert from 'node:assert/strict'
import { test } from 'node:test'
import { allowedText, lastReceivedText, pairingInstruction, telegramNameOk, telegramStateText, telegramTokenOk } from './channels.ts'

test('names and tokens follow the host rules', () => {
  assert.ok(telegramNameOk('home') && telegramNameOk('work-2'))
  for (const bad of ['', 'Home', 'a b', 'x'.repeat(33)]) assert.ok(!telegramNameOk(bad), bad)
  assert.ok(telegramTokenOk('123456789:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_'))
  for (const bad of ['', '123', 'abc:AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw_', '1:short']) assert.ok(!telegramTokenOk(bad), bad)
})

test('the state of a Telegram bot is worded for a person', () => {
  assert.equal(telegramStateText('ok'), 'Connected')
  assert.equal(telegramStateText('starting'), 'Starting…')
  assert.equal(telegramStateText('error: Telegram getUpdates failed: timed out'), 'Cannot reach Telegram')
})

test('the last message is told in the nearest unit', () => {
  const now = 10 * 24 * 3_600_000
  assert.equal(lastReceivedText(null, now), 'Nothing received yet')
  assert.equal(lastReceivedText(now - 20_000, now), 'Last message just now')
  assert.equal(lastReceivedText(now - 5 * 60_000, now), 'Last message 5 min ago')
  assert.equal(lastReceivedText(now - 3 * 3_600_000, now), 'Last message 3 h ago')
  assert.equal(lastReceivedText(now - 2 * 24 * 3_600_000, now), 'Last message 2 d ago')
  assert.equal(lastReceivedText(now + 5000, now), 'Last message just now')
})

test('pairing tells where to send the code', () => {
  assert.equal(pairingInstruction('ABCD2345', 'home_bot'), 'Send /start ABCD2345 to @home_bot in Telegram. The code works once, for 10 minutes.')
  assert.match(pairingInstruction('ABCD2345', ''), /your Telegram bot/)
  assert.equal(allowedText(1), '1 person allowed')
  assert.equal(allowedText(0), '0 people allowed')
})
