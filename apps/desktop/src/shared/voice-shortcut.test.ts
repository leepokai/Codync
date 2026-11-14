import assert from 'node:assert/strict'
import { test } from 'node:test'
import { CommandTaps, VoiceKeyRouter, type ModifierChange } from './voice-shortcut.ts'

const COMMAND = 0x100000
const RIGHT = 0x10
const LEFT = 0x08
const down = (at: number, sinceInput = 60, flags = COMMAND | RIGHT): ModifierChange => ({ key: 54, flags, at, sinceInput })
const up = (at: number, sinceInput = 60, flags = 0): ModifierChange => ({ key: 54, flags, at, sinceInput })
const feed = (changes: ModifierChange[], taps = new CommandTaps('rightCommand')) => changes.map((c) => taps.feed(c))

test('a quick tap is a tap, and a second one right after is a double tap', () => {
  assert.deepEqual(feed([down(10), up(10.08), down(10.25), up(10.33)]), [null, 'tap', null, 'double'])
  // The next tap starts over: it sends what was said.
  assert.deepEqual(feed([down(10), up(10.08), down(10.25), up(10.33), down(12), up(12.08)]).at(-1), 'tap')
  // Double-tapping to stop: the tap that sends and the next pair up (VoiceKeyRouter ignores that double).
  assert.deepEqual(feed([down(10), up(10.08), down(10.25), up(10.33), down(12), up(12.08), down(12.3), up(12.38)]).slice(3), ['double', null, 'tap', null, 'double'])
})

test('slow taps are two taps, and a long press is none', () => {
  assert.deepEqual(feed([down(10), up(10.08), down(11), up(11.08)]), [null, 'tap', null, 'tap'])
  assert.deepEqual(feed([down(10), up(10.5), down(10.6), up(10.68)]), [null, null, null, 'tap'])
})

test('a key pressed with the command, or between the taps, is a shortcut', () => {
  // ⌘C then ⌘V with the right command.
  assert.deepEqual(feed([down(10), up(10.2, 0.1), down(10.3), up(10.45, 0.05)]), [null, null, null, null])
  // A key between the taps.
  assert.equal(feed([down(10), up(10.08), down(10.25), up(10.33, 0.15)]).at(-1), 'tap')
})

test('the other command and other modifiers break a tap', () => {
  assert.deepEqual(feed([down(10, 60, COMMAND | RIGHT | LEFT), up(10.08, 60, COMMAND | LEFT)]), [null, null])
  assert.equal(feed([down(10), up(10.08), { key: 56, flags: 0x20002, at: 10.15, sinceInput: 60 }, down(10.25), up(10.33)]).at(-1), 'tap')
  assert.equal(feed([down(10), up(10.08), down(10.25, 60, COMMAND | RIGHT | 0x20000), up(10.33)]).at(-1), null)
})

test('the left command works the same way, and the right one then does nothing', () => {
  const left = (at: number, isDown: boolean): ModifierChange => ({ key: 55, flags: isDown ? COMMAND | LEFT : 0, at, sinceInput: 60 })
  assert.deepEqual(feed([left(10, true), left(10.08, false), left(10.25, true), left(10.33, false)], new CommandTaps('leftCommand')), [null, 'tap', null, 'double'])
  assert.deepEqual(feed([down(10), up(10.08)], new CommandTaps('leftCommand')), [null, null])
})

test('a double tap starts dictation, the next tap sends, and the double tap that ends on it starts nothing', () => {
  const keys = new VoiceKeyRouter()
  const idle = { byShortcut: false, busy: false }
  assert.equal(keys.route('double', idle), 'start')
  assert.equal(keys.route('tap', { byShortcut: true, busy: true }), 'send')
  assert.equal(keys.route('double', idle), null)
  assert.equal(keys.route('double', idle), 'start')
  // A tap with no shortcut start (the mic, or after the send) does nothing; nor does a double tap while busy.
  assert.equal(keys.route('tap', { byShortcut: false, busy: true }), null)
  assert.equal(keys.route('double', { byShortcut: false, busy: true }), null)
})
