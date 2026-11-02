import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { HostEvent } from '../client/host-client'
import { CatchUp } from './catch-up.ts'

const hello = (rev: number, catchUp: number) => ({ type: 'hello', hostId: 'h', rev, usage: {}, screen: null, network: null, catchUp }) as unknown as HostEvent
const icons = { type: 'icons', iconsRevision: 1, icons: {} } as unknown as HostEvent
const bot = { type: 'bot', bot: { id: 'a', rev: 3 } } as unknown as HostEvent

test('catch-up ends on its last replayed event even when the host rev is ahead of all of them', () => {
  const catchUp = new CatchUp()
  assert.deepEqual([hello(9, 2), icons, bot].map((e) => catchUp.ended(e)), [false, false, true])
})

test('a later live event never ends a catch-up that already ended', () => {
  const catchUp = new CatchUp()
  assert.deepEqual([hello(9, 0), bot, bot].map((e) => catchUp.ended(e)), [true, false, false])
})

test('a reset stream waits for its own hello', () => {
  const catchUp = new CatchUp()
  catchUp.ended(hello(9, 2))
  catchUp.reset()
  assert.deepEqual([bot, bot].map((e) => catchUp.ended(e)), [false, false])
})
