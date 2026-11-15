import assert from 'node:assert/strict'
import { test } from 'node:test'
import { chatWindowMinWidth } from './chat-layout.ts'

test('the chat window fits a 1080 × 1920 portrait display scaled to 720 points wide (#359)', () => {
  assert.ok(chatWindowMinWidth <= 720)
})
