import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { HostEvent } from '../client/host-client'
import { CatchUp, readCatchUp } from './catch-up.ts'

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

test('replaying holds from a counted hello until its last replayed event', () => {
  const catchUp = new CatchUp()
  const replaying = [hello(9, 2), icons, bot].map((e) => (catchUp.ended(e), catchUp.replaying))
  assert.deepEqual(replaying, [true, true, false])
  catchUp.ended(hello(9, 0))
  assert.equal(catchUp.replaying, false)
})

test('a hello without a count never holds reads back', () => {
  const catchUp = new CatchUp()
  catchUp.ended({ type: 'hello' } as unknown as HostEvent)
  assert.equal(catchUp.replaying, false)
})

test('a raw hello carries its catchUp count through to the counter', () => {
  const raw = JSON.parse('{"type":"hello","hostId":"h","rev":9,"catchUp":1}') as Record<string, unknown>
  const catchUp = new CatchUp()
  assert.deepEqual([{ type: 'hello', catchUp: readCatchUp(raw) } as unknown as HostEvent, bot].map((e) => catchUp.ended(e)), [false, true])
})

test('a hello from a host before the count leaves catch-up open', () => {
  assert.equal(readCatchUp({ type: 'hello', rev: 9 }), undefined)
})
