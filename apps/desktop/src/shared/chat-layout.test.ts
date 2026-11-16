import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import { test } from 'node:test'
import { chatWindowMinHeight, chatWindowMinWidth, growToMinimum, windowMinimumFor } from './chat-layout.ts'

test('the chat window fits a 1080 × 1920 portrait display scaled to 720 points wide (#359)', () => {
  assert.ok(chatWindowMinWidth <= 720)
})

test('chat-window.css repeats the window minimum, so keep the two equal', async () => {
  const css = await readFile(new URL('../renderer/views/chat-window.css', import.meta.url), 'utf8')
  assert.match(css, new RegExp(`min-width:\\s*${chatWindowMinWidth}px`))
  assert.match(css, new RegExp(`min-height:\\s*${chatWindowMinHeight}px`))
})

test('the window minimum leaves the page its floor once the frame and menu bar are counted (#476)', () => {
  // Windows at the OS minimum: a 696 × 565 window around a 680 × 500 page.
  const minimum = windowMinimumFor({ width: chatWindowMinWidth, height: chatWindowMinHeight }, { width: 696, height: 565 })
  assert.deepEqual(minimum, { width: 696, height: 565 })
  // The default 1100 × 760 window shows the same 16 × 65 of chrome around a 1084 × 695 page.
  assert.deepEqual(windowMinimumFor({ width: 1084, height: 695 }, { width: 1100, height: 760 }), minimum)
  // Taking the measured chrome off the minimum gives the page floor back, not less.
  assert.equal(minimum.width - (696 - chatWindowMinWidth), chatWindowMinWidth)
  assert.equal(minimum.height - (565 - chatWindowMinHeight), chatWindowMinHeight)
})

test('a page that fills the window keeps the floor, and a window already larger keeps its size (#476)', () => {
  assert.deepEqual(windowMinimumFor({ width: 1100, height: 760 }, { width: 1100, height: 760 }), {
    width: chatWindowMinWidth,
    height: chatWindowMinHeight,
  })
  const minimum = { width: 696, height: 565 }
  assert.deepEqual(growToMinimum({ width: 1100, height: 760 }, minimum), { width: 1100, height: 760 })
  assert.deepEqual(growToMinimum({ width: 680, height: 500 }, minimum), minimum)
  assert.deepEqual(growToMinimum({ width: 1100, height: 500 }, minimum), { width: 1100, height: 565 })
})
