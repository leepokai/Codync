import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { test } from 'node:test'
import { chatWindowMinHeight, chatWindowMinWidth } from './chat-layout.ts'

test('the chat window fits a 1080 × 1920 portrait display scaled to 720 points wide (#359)', () => {
  assert.ok(chatWindowMinWidth <= 720)
})

test('chat-window.css repeats the window minimum, so keep the two equal', async () => {
  const css = await readFile(new URL('../renderer/views/chat-window.css', import.meta.url), 'utf8')
  assert.match(css, new RegExp(`min-width:\\s*${chatWindowMinWidth}px`))
  assert.match(css, new RegExp(`min-height:\\s*${chatWindowMinHeight}px`))
})
